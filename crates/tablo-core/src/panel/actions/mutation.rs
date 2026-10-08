//! The one pipeline every mutation outside the forms runs through: one record's and the bulk
//! bar's Delete, the custom actions of either, and the header actions of a list or a page.
//!
//! [`mutate`] owns ADR-0004's invariant: the resource-wide ability (`DeleteAny`, or `RunAny` for a
//! custom action), the CSRF check and the confirmation marker before any DB work, then one
//! transaction that loads the targets through the tenant-scoped query, checks `View` on every one,
//! runs the entry's per-record check (`Delete`, or `Run` and `can_run`), writes the records that
//! pass, and commits. [`run_header`] runs a header action through the same reading of the POST
//! and the same transaction, with no record to load.
//!
//! An action that asks for input stops before the write until its input parses: the POST without
//! the input page's marker renders that page, and one whose input is refused renders it again with
//! the errors. The input is parsed, validated and checked before the transaction opens, as a record
//! form's submission is, and its relationship choices are re-checked inside it. A page renders
//! only after the records pass every check, and rolls the transaction back unwritten.

use std::{collections::HashMap, future::Future, pin::Pin};

use topcoat::{
    context::Cx,
    router::{
        Body,
        error::{bad_request, forbidden, not_found, see_other},
        path_param_segment,
    },
    view::BoxView,
};

use super::{
    super::{
        forms::{FormChrome, parse_form_body, render_form_page, truthy},
        gate::{gate, landing_url},
        write::{commit_then, commit_write},
    },
    fetch::{composite_pk_error, find_by_key},
};
use crate::{
    db::db,
    form::FieldErrors,
    notification::{Notification, set_notification},
    policy::Ability,
    resource::{
        ActionEntry, Committed, ErasedInput, HeaderEntry, InputSpec, Mounted, Places,
        RESERVED_KEYS, Resource, SUBMITTED_KEY, run_after_commit,
    },
    topcoat_compat::async_page,
};

/// Which records a mutation POST names.
#[derive(Clone, Copy)]
enum Target {
    /// The one record in the URL: `{list}/{key}/delete`, `{list}/{key}/-/actions/{name}`.
    Record,
    /// The `ids` the bulk form carried: `{list}/bulk-delete`, `{list}/-/actions/{name}`.
    Bulk,
}

/// One record's Delete POST, from its row or its detail or edit page: `{list}/{key}/delete`.
///
/// The confirmation is an alert dialog: the Delete button opens it on this route, and its form
/// POSTs here with `confirm=1`. The dialog is deliberately fetch-free and policy-blind: it carries
/// no record data and embeds only the caller's own CSRF token, and the policy and tenancy checks
/// run against the loaded record here.
pub(crate) fn resource_delete<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        mutate(
            cx,
            &resource,
            body,
            Target::Record,
            Some(ActionEntry::delete()),
        )
        .await
    })
}

/// The bulk bar's Delete POST: `{list}/bulk-delete`, with the selection in the `ids` field.
pub(crate) fn resource_bulk_delete<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let action = Some(ActionEntry::bulk_delete());
        mutate(cx, &resource, body, Target::Bulk, action).await
    })
}

/// One record's custom action POST, from its row or its detail or edit page:
/// `{list}/{key}/-/actions/{name}`.
pub(crate) fn resource_record_action<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let action = custom_action(cx, &resource, Places::RECORD);
        mutate(cx, &resource, body, Target::Record, action).await
    })
}

/// The list's action POST, `{list}/-/actions/{name}`: a header action, or the bulk bar's custom
/// action with the selection in the `ids` field.
pub(crate) fn resource_list_action<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let name = path_param_segment(cx, "action");
        if let Some(header) = resource.header_actions.find(name).copied() {
            // The button renders on the list only, so the POST asks what the list asks first.
            let allowed = resource.can(cx, Ability::ViewAny)
                && resource.can(
                    cx,
                    Ability::RunHeader {
                        action: header.name,
                    },
                )
                && (header.can_run)(cx);
            if !allowed {
                return Err(forbidden().into());
            }
            let after: AfterCommit = after_header_commit::<R>;
            return run_header(cx, body, header, &resource.url, Some(after)).await;
        }
        let action = custom_action(cx, &resource, Places::BULK);
        mutate(cx, &resource, body, Target::Bulk, action).await
    })
}

/// The declared action the `{name}` path segment names, if placed on any of `places`.
fn custom_action<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    places: Places,
) -> Option<ActionEntry<R>> {
    let name = path_param_segment(cx, "action");
    resource
        .actions
        .find(name)
        .filter(|a| a.places.intersects(places))
        .copied()
}

/// What a header action's POST runs once its write commits.
pub(crate) type AfterCommit =
    for<'c> fn(&'c Cx, &'static str) -> Pin<Box<dyn Future<Output = ()> + Send + 'c>>;

/// `R`'s [`Resource::after_commit`] for its header action `name`.
fn after_header_commit<'c, R: Resource>(
    cx: &'c Cx,
    name: &'static str,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'c>> {
    Box::pin(run_after_commit::<R>(cx, Committed::ran(name)))
}

/// What the POST of an action asked for, read and checked before the transaction opens.
struct Posted {
    /// The whole submission: the input, the CSRF token, the confirmation and the selection.
    values: HashMap<String, String>,
    pending: Pending,
}

/// Reads the POST of the action `name`: its CSRF token, its confirmation when `confirm` asks for
/// one and the POST would write, and its input.
///
/// The confirmation UI is an alert dialog, or the input page of an action with input, whose forms
/// carry `confirm=1`, so a missing marker is a malformed client, not a user path. The POST that
/// opens an input page writes nothing and needs none.
async fn read_post(
    cx: &Cx,
    body: Body,
    name: &str,
    confirm: bool,
    input: &InputSpec,
) -> Result<Posted, topcoat::Error> {
    // A mutation carries no file parts: only the values half is read.
    let values = parse_form_body(cx, body).await?.values;
    crate::csrf::verify(cx, &values)?;
    let writes = !input.takes_input || values.contains_key(SUBMITTED_KEY);
    if confirm && writes && !values.get("confirm").is_some_and(|v| truthy(v)) {
        return Err(bad_request(format!("{name} requires confirmation")).into());
    }
    // The input's checks may query, so they run before the transaction holds a connection.
    let pending = read_input(cx, name, input, &values).await?;
    Ok(Posted { values, pending })
}

/// Runs `action` on the records `target` names.
///
/// The resource-wide ability, the CSRF check and the confirmation marker run before the
/// transaction opens: a forged POST answers 403 without holding a pooled connection across the
/// body read or probing record existence.
async fn mutate<'a, R: Resource>(
    cx: &'a Cx,
    resource: &Mounted<R>,
    body: Body,
    target: Target,
    action: Option<ActionEntry<R>>,
) -> Result<BoxView<'a>, topcoat::Error> {
    // An unknown action asks `ViewAny`, so a caller who may not open the list gets the 403 a
    // refused action gets, and the 404 names no action to them.
    let resource_wide = action.map_or(Ability::ViewAny, |action| action.resource_wide);
    if !resource.can(cx, resource_wide) {
        return Err(forbidden().into());
    }
    let Some(action) = action else {
        return Err(not_found().into());
    };
    let Posted { values, pending } =
        read_post(cx, body, action.name, action.confirm, &action.input).await?;
    let ids = match target {
        Target::Record => vec![path_param_segment(cx, "id").to_string()],
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
    // The fetched snapshot is the checked snapshot: the load, the checks and the write share
    // the transaction, and any error rolls it back.
    let mut db = db(cx);
    let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
    let rows = load_targets(cx, resource, &ids, target, &mut tx).await?;
    // A record that cannot be viewed cannot be written by guessing its key.
    if rows.iter().any(|row| !resource.can(cx, Ability::View(row))) {
        return Err(forbidden().into());
    }
    let (rows, refused): (Vec<R::Model>, Vec<R::Model>) = rows
        .into_iter()
        .partition(|row| (action.can_run)(resource, cx, row));
    if rows.is_empty() {
        return Err(refuse(cx, resource, &action, target, refused.len()));
    }
    let skipped = (!refused.is_empty()).then(|| {
        let skipped = refused.len();
        let selected = rows.len() + skipped;
        format!(" ({skipped} of {selected} skipped)")
    });
    let label = (action.label)(cx);
    let page = |page: InputPage, label: String| {
        let title = match target {
            Target::Record => format!("{label}: {}", resource.record_title(&rows[0], &ids[0])),
            Target::Bulk => {
                let count = rows.len();
                let noun = if count == 1 { "record" } else { "records" };
                format!(
                    "{label}: {count} {noun}{}",
                    skipped.as_deref().unwrap_or_default()
                )
            }
        };
        let mut hidden = Vec::new();
        if let (Target::Bulk, Some(ids)) = (target, values.get("ids")) {
            hidden.push(("ids".to_string(), ids.clone()));
        }
        let chrome = page.chrome(title, label, action.confirm, hidden, &resource.url);
        (page, chrome)
    };
    let input = match pending {
        Pending::Ready(input) => input,
        Pending::Submitted {
            input,
            values: posted,
        } => {
            let errors = (action.input.schema)()
                .recheck_relationships(cx, &posted, &mut tx)
                .await;
            if !errors.is_empty() {
                let (page, chrome) = page(
                    InputPage {
                        values: posted,
                        errors,
                    },
                    label,
                );
                // The page's choices may query: release the connection first.
                drop(tx);
                return page.render(cx, &action.input, chrome).await;
            }
            input
        }
        Pending::Page(input_page) => {
            let (input_page, chrome) = page(input_page, label);
            drop(tx);
            return input_page.render(cx, &action.input, chrome).await;
        }
    };
    let mut note = (action.success)(cx, rows.len());
    note.push_str(skipped.as_deref().unwrap_or_default());
    // A delete's hook names what was removed: the pre-delete snapshot.
    let written = (action.run)(cx, &rows, input, &mut tx).await.map(|()| rows);
    commit_write(
        cx,
        resource,
        tx,
        written,
        action.acted,
        note,
        action.failure,
    )
    .await
}

/// Runs the header action `action`, whose resource-wide or page check already passed, landing on
/// `home` or the POST's `?return=`, and runs `after` once its write commits.
pub(crate) async fn run_header<'a>(
    cx: &'a Cx,
    body: Body,
    action: HeaderEntry,
    home: &str,
    after: Option<AfterCommit>,
) -> Result<BoxView<'a>, topcoat::Error> {
    let Posted { pending, .. } =
        read_post(cx, body, action.name, action.confirm, &action.input).await?;
    let render = |page: InputPage| {
        let label = (action.label)(cx);
        let chrome = page.chrome(label.clone(), label, action.confirm, Vec::new(), home);
        (page, chrome)
    };
    let (input, submitted) = match pending {
        Pending::Ready(input) => (input, None),
        Pending::Submitted { input, values } => (input, Some(values)),
        Pending::Page(page) => {
            let (page, chrome) = render(page);
            return page.render(cx, &action.input, chrome).await;
        }
    };
    let mut db = db(cx);
    let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
    if let Some(values) = submitted {
        let errors = (action.input.schema)()
            .recheck_relationships(cx, &values, &mut tx)
            .await;
        if !errors.is_empty() {
            // The page's choices may query: release the connection first.
            drop(tx);
            let (page, chrome) = render(InputPage { values, errors });
            return page.render(cx, &action.input, chrome).await;
        }
    }
    let written = (action.run)(cx, input, &mut tx).await;
    let note = (action.success)(cx);
    commit_then(
        cx,
        tx,
        written,
        |()| async move {
            if let Some(after) = after {
                after(cx, action.name).await;
            }
        },
        note,
        "run the action",
        home,
    )
    .await
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

/// Reads the input of the action `name` from the POST before the transaction opens.
///
/// # Errors
///
/// A submission holding a key the input does not declare answers 400.
async fn read_input(
    cx: &Cx,
    name: &str,
    spec: &InputSpec,
    values: &HashMap<String, String>,
) -> Result<Pending, topcoat::Error> {
    if !spec.takes_input {
        // Mounting refuses an input with no field whose parse refuses an empty submission.
        let input = (spec.parse)(cx, &HashMap::new()).map_err(|_| {
            crate::error::declaration(format!(
                "action '{name}' declares no input field but refuses an empty input"
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
    let schema = (spec.schema)();
    let unknown = schema.unknown_keys(&input);
    if !unknown.is_empty() {
        return Err(bad_request(format!("unknown field(s): {}", unknown.join(", "))).into());
    }
    let mut errors = FieldErrors::new();
    let parsed = match (spec.parse)(cx, &input) {
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
    /// The page's chrome: titled `title`, submitted by `label`, carrying the input's marker, the
    /// confirmation and `hidden` back to this route, and cancelled to `home` or the `?return=`.
    fn chrome(
        &self,
        title: String,
        label: String,
        confirm: bool,
        hidden: Vec<(String, String)>,
        home: &str,
    ) -> FormChrome {
        let mut carried = vec![(SUBMITTED_KEY.to_string(), "1".to_string())];
        if confirm {
            carried.push(("confirm".to_string(), "1".to_string()));
        }
        carried.extend(hidden);
        FormChrome::action(title, label, confirm, carried, home.to_string())
    }

    /// Renders the page of the input `spec` with `chrome`.
    async fn render<'a>(
        self,
        cx: &'a Cx,
        spec: &InputSpec,
        chrome: FormChrome,
    ) -> topcoat::Result<BoxView<'a>> {
        render_form_page(
            cx,
            &(spec.schema)(),
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
    if let Target::Record = target {
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
        Target::Record => forbidden().into(),
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
