//! The one pipeline every record mutation outside the forms runs through: the row's and the bulk
//! bar's Delete, and the custom actions of either.
//!
//! [`run_mutation`] owns ADR-0004's invariant: the resource-wide ability (`DeleteAny`, or
//! `RunAny` for a custom action), the CSRF check and the confirmation marker before any DB work,
//! then one transaction that loads the targets through the tenant-scoped query, checks `View` on
//! every one, runs the entry's per-record check (`Delete`, or `Run` and `can_run`), writes the
//! records that pass, and commits.
//!
//! An action that asks for input stops before the write until its input parses: the POST without
//! the input page's marker renders that page, and one whose input is refused renders it again with
//! the errors. The input is parsed, validated and checked before the transaction opens, as a record
//! form's submission is, and its relationship choices are re-checked inside it. A page renders
//! only after the records pass every check, and rolls the transaction back unwritten.

use std::collections::HashMap;

use topcoat::{
    context::Cx,
    router::{
        Body,
        error::{bad_request, forbidden, not_found, see_other},
    },
    view::BoxView,
};

use super::{
    super::{
        forms::{FormChrome, parse_form_body, render_form_page, truthy},
        gate::{gate, landing_url},
        write::commit_write,
    },
    fetch::{composite_pk_error, find_by_key},
};
use crate::{
    db::db,
    form::FieldErrors,
    notification::{Notification, set_notification},
    policy::Ability,
    resource::{ActionEntry, ErasedInput, Mounted, RESERVED_KEYS, Resource, SUBMITTED_KEY},
    topcoat_compat::async_page,
};

/// Which records a mutation POST names.
#[derive(Clone, Copy)]
enum Target {
    /// The one record in the URL: `{list}/{key}/delete`, `{list}/{key}/-/actions/{name}`.
    Row,
    /// The `ids` the bulk form carried: `{list}/bulk-delete`, `{list}/-/actions/{name}`.
    Bulk,
}

/// The row's Delete POST: `{list}/{key}/delete`.
///
/// The confirmation is the table's alert dialog: the row's Delete opens it on this route, and its
/// form POSTs here with `confirm=1`. The dialog is deliberately fetch-free and policy-blind: it
/// carries no record data and embeds only the caller's own CSRF token, and the policy and tenancy
/// checks run against the loaded record here.
pub(crate) fn resource_delete<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    run_mutation::<R>(cx, body, Target::Row, |_| Some(ActionEntry::delete()))
}

/// The bulk bar's Delete POST: `{list}/bulk-delete`, with the selection in the `ids` field.
pub(crate) fn resource_bulk_delete<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    run_mutation::<R>(cx, body, Target::Bulk, |_| Some(ActionEntry::bulk_delete()))
}

/// A row's custom action POST: `{list}/{key}/-/actions/{name}`.
pub(crate) fn resource_row_action<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    run_mutation::<R>(cx, body, Target::Row, |resource| {
        custom_action(cx, resource, |action| action.row)
    })
}

/// The bulk bar's custom action POST: `{list}/-/actions/{name}`, with the selection in the `ids`
/// field.
pub(crate) fn resource_bulk_action<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    run_mutation::<R>(cx, body, Target::Bulk, |resource| {
        custom_action(cx, resource, |action| action.bulk)
    })
}

/// The declared action the `{name}` path segment names, if `offered` on this route.
fn custom_action<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    offered: fn(&ActionEntry<R>) -> bool,
) -> Option<ActionEntry<R>> {
    let name = topcoat::router::path_param_segment(cx, "action");
    resource.actions.find(name).filter(|a| offered(a)).copied()
}

/// Runs the mutation `select` picks on the records `target` names.
///
/// The resource-wide ability, the CSRF check and the confirmation marker run before the
/// transaction opens: a forged POST answers 403 without holding a pooled connection across the
/// body read or probing record existence.
fn run_mutation<'a, R: Resource>(
    cx: &'a Cx,
    body: Body,
    target: Target,
    select: impl FnOnce(&Mounted<R>) -> Option<ActionEntry<R>> + Send + 'a,
) -> BoxView<'a> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let action = select(&resource);
        // An unknown action asks `ViewAny`, so a caller who may not open the list gets the 403 a
        // refused action gets, and the 404 names no action to them.
        let resource_wide = action.map_or(Ability::ViewAny, |action| action.resource_wide);
        if !resource.can(cx, resource_wide) {
            return Err(forbidden().into());
        }
        let Some(action) = action else {
            return Err(not_found().into());
        };
        // A mutation carries no file parts: only the values half is read.
        let values = parse_form_body(cx, body).await?.values;
        crate::csrf::verify(cx, &values)?;
        // The confirmation UI is the table's alert dialog, or the input page of an action with
        // input, whose forms carry `confirm=1`, so a missing marker is a malformed client, not a
        // user path. The POST that opens an input page writes nothing and needs none.
        let writes = !action.takes_input || values.contains_key(SUBMITTED_KEY);
        if action.confirm && writes && !values.get("confirm").is_some_and(|v| truthy(v)) {
            return Err(bad_request(format!("{} requires confirmation", action.name)).into());
        }
        let ids = match target {
            Target::Row => vec![topcoat::router::path_param_segment(cx, "id").to_string()],
            Target::Bulk => {
                let raw = values.get("ids").cloned().unwrap_or_default();
                let ids = parse_bulk_ids(&raw, MAX_BULK_IDS);
                if ids.is_empty() {
                    set_notification(cx, Notification::error("Select at least one row first"));
                    return Err(see_other(landing_url(cx, &resource.url)).into());
                }
                if ids.len() > MAX_BULK_IDS {
                    return Err(bad_request(format!("too many ids (max {MAX_BULK_IDS})")).into());
                }
                ids
            }
        };
        // The input's checks may query, so they run before the transaction holds a connection.
        let pending = read_input(cx, &action, &values).await?;
        // The fetched snapshot is the checked snapshot: the load, the checks and the write share
        // the transaction, and any error rolls it back.
        let mut db = db(cx);
        let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
        let rows = load_targets(cx, &resource, &ids, target, &mut tx).await?;
        // A record that cannot be viewed cannot be written by guessing its key.
        if rows.iter().any(|row| !resource.can(cx, Ability::View(row))) {
            return Err(forbidden().into());
        }
        let (rows, refused): (Vec<R::Model>, Vec<R::Model>) = rows
            .into_iter()
            .partition(|row| (action.can_run)(&resource, cx, row));
        if rows.is_empty() {
            return Err(refuse(cx, &resource, &action, target, refused.len()));
        }
        let skipped = (!refused.is_empty()).then(|| {
            let skipped = refused.len();
            let selected = rows.len() + skipped;
            format!(" ({skipped} of {selected} skipped)")
        });
        let title = || {
            let label = (action.label)(cx);
            match target {
                Target::Row => format!("{label}: {}", resource.record_title(&rows[0], &ids[0])),
                Target::Bulk => {
                    let count = rows.len();
                    let noun = if count == 1 { "record" } else { "records" };
                    format!(
                        "{label}: {count} {noun}{}",
                        skipped.as_deref().unwrap_or_default()
                    )
                }
            }
        };
        let input = match pending {
            Pending::Ready(input) => input,
            Pending::Submitted {
                input,
                values: posted,
            } => {
                let schema = (action.input)();
                let errors = schema.recheck_relationships(cx, &posted, &mut tx).await;
                if !errors.is_empty() {
                    let page = InputPage {
                        values: posted,
                        errors,
                    };
                    let title = title();
                    // The page's choices may query: release the connection first.
                    drop(tx);
                    return page
                        .render(cx, &resource, &action, target, &values, title)
                        .await;
                }
                input
            }
            Pending::Page(page) => {
                let title = title();
                drop(tx);
                return page
                    .render(cx, &resource, &action, target, &values, title)
                    .await;
            }
        };
        let mut note = (action.success)(cx, rows.len());
        note.push_str(skipped.as_deref().unwrap_or_default());
        // A delete's hook names what was removed: the pre-delete snapshot.
        let written = (action.run)(cx, &rows, input, &mut tx).await.map(|()| rows);
        commit_write(
            cx,
            &resource,
            tx,
            written,
            action.acted,
            note,
            action.failure,
        )
        .await
    })
}

/// The input page an action renders before it runs: blank, or holding a refused submission.
struct InputPage {
    values: HashMap<String, String>,
    errors: FieldErrors,
}

/// What the POST says about the action's input.
enum Pending {
    /// An action that asks for nothing, with the `()` it parsed from nothing.
    Ready(ErasedInput),
    /// A submission that parsed, validated and passed its checks, with its values for the
    /// relationship re-check inside the transaction.
    Submitted {
        input: ErasedInput,
        values: HashMap<String, String>,
    },
    /// The input page to render instead of running: blank, or holding a refused submission.
    Page(InputPage),
}

/// Reads the action's input from the POST before the transaction opens.
///
/// # Errors
///
/// A submission holding a key the input does not declare answers 400.
async fn read_input<R: Resource>(
    cx: &Cx,
    action: &ActionEntry<R>,
    values: &HashMap<String, String>,
) -> Result<Pending, topcoat::Error> {
    if !action.takes_input {
        // Mounting refuses an input with no field whose parse refuses an empty submission.
        let input = (action.parse_input)(cx, &HashMap::new()).map_err(|_| {
            crate::error::declaration(format!(
                "action '{}' declares no input field but refuses an empty input",
                action.name
            ))
        })?;
        return Ok(Pending::Ready(input));
    }
    if !values.contains_key(SUBMITTED_KEY) {
        return Ok(Pending::Page(InputPage {
            values: HashMap::new(),
            errors: FieldErrors::new(),
        }));
    }
    let mut input = values.clone();
    input.retain(|key, _| !RESERVED_KEYS.contains(&key.as_str()));
    let schema = (action.input)();
    let unknown = schema.unknown_keys(&input);
    if !unknown.is_empty() {
        return Err(bad_request(format!("unknown field(s): {}", unknown.join(", "))).into());
    }
    let mut errors = FieldErrors::new();
    let parsed = match (action.parse_input)(cx, &input) {
        Ok(parsed) => Some(parsed),
        Err(refused) => {
            for error in refused {
                errors.push(error);
            }
            None
        }
    };
    schema.check_controls(cx, &input, &mut errors).await;
    match parsed {
        Some(parsed) if errors.is_empty() => Ok(Pending::Submitted {
            input: parsed,
            values: input,
        }),
        _ => Ok(Pending::Page(InputPage {
            values: input,
            errors,
        })),
    }
}

impl InputPage {
    /// Renders the page titled `title`, whose submit POSTs back to this route with the input's
    /// marker, the confirmation and the selection.
    async fn render<'a, R: Resource>(
        self,
        cx: &'a Cx,
        resource: &Mounted<R>,
        action: &ActionEntry<R>,
        target: Target,
        posted: &HashMap<String, String>,
        title: String,
    ) -> topcoat::Result<BoxView<'a>> {
        let mut hidden = vec![(SUBMITTED_KEY.to_string(), "1".to_string())];
        if action.confirm {
            hidden.push(("confirm".to_string(), "1".to_string()));
        }
        if let (Target::Bulk, Some(ids)) = (target, posted.get("ids")) {
            hidden.push(("ids".to_string(), ids.clone()));
        }
        let chrome =
            FormChrome::action(resource, title, (action.label)(cx), action.confirm, hidden);
        render_form_page(
            cx,
            &(action.input)(),
            chrome,
            &self.values,
            &self.errors,
            &Default::default(),
        )
        .await
    }
}

/// Loads the records `ids` names through the tenant-scoped query inside the transaction, 404ing
/// unless every one is found.
async fn load_targets<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    ids: &[String],
    target: Target,
    tx: &mut toasty::Transaction<'_>,
) -> Result<Vec<R::Model>, topcoat::Error> {
    if let Target::Row = target {
        return Ok(vec![find_by_key(cx, resource, &ids[0], tx).await?]);
    }
    let keys: Vec<&str> = ids.iter().map(String::as_str).collect();
    let Some(pk_filter) = crate::toasty_compat::pk::pk_in_expr::<R::Model>(&keys) else {
        if let Some(error) = composite_pk_error(resource) {
            return Err(error);
        }
        return Err(not_found().into());
    };
    let rows = resource
        .scoped_query(cx)?
        .filter(pk_filter)
        .exec(tx)
        .await
        .map_err(crate::error::unavailable)?;
    if rows.len() != ids.len() {
        return Err(not_found().into());
    }
    Ok(rows)
}

/// Answers a target the mutation refuses on every record: a row with 403, a selection with an
/// error notification naming `refused` records.
fn refuse<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    action: &ActionEntry<R>,
    target: Target,
    refused: usize,
) -> topcoat::Error {
    match target {
        Target::Row => forbidden().into(),
        Target::Bulk => {
            let noun = if refused == 1 { "record" } else { "records" };
            set_notification(
                cx,
                Notification::error(format!(
                    "{}: {refused} selected {noun} cannot take this action; nothing was changed",
                    (action.label)(cx)
                )),
            );
            see_other(landing_url(cx, &resource.url)).into()
        }
    }
}

/// Bounds a selection's `IN` list.
pub(super) const MAX_BULK_IDS: usize = 400;

/// Parses and dedupes bulk `ids` while preserving order, stopping at `max + 1` so the caller can
/// answer 400.
pub(super) fn parse_bulk_ids(raw: &str, max: usize) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for s in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if seen.insert(s) {
            ids.push(s.to_string());
            if ids.len() > max {
                break;
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests;
