//! The one pipeline every record mutation outside the forms runs through: the row's and the bulk
//! bar's Delete, and the custom actions of either.
//!
//! [`run_mutation`] owns ADR-0004's invariant: the resource-wide ability (`DeleteAny`, or
//! `RunAny` for a custom action), the CSRF check and the confirmation marker before any DB work,
//! then one transaction that loads the targets through the tenant-scoped query, checks `View` on
//! every one, runs the entry's per-record check (`Delete`, or `Run` and `can_run`), writes the
//! records that pass, and commits.

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
        forms::{parse_form_body, truthy},
        gate::{gate, landing_url},
        write::commit_write,
    },
    fetch::{composite_pk_error, find_by_key},
};
use crate::{
    db::db,
    notification::{Notification, set_notification},
    policy::Ability,
    resource::{ActionEntry, Mounted, Resource},
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
        // The confirmation UI is the table's alert dialog, whose form carries `confirm=1`, so a
        // missing marker is a malformed client, not a user path.
        if action.confirm && !values.get("confirm").is_some_and(|v| truthy(v)) {
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
        let mut note = (action.success)(cx, rows.len());
        if !refused.is_empty() {
            let skipped = refused.len();
            let selected = rows.len() + skipped;
            note.push_str(&format!(" ({skipped} of {selected} skipped)"));
        }
        // A delete's hook names what was removed: the pre-delete snapshot.
        let written = (action.run)(cx, &rows, &mut tx).await.map(|()| rows);
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
