//! Custom action POSTs: one record from its row, or the selection from the
//! bulk bar, policy-checked and run in the framework transaction.

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
        forms::parse_form_body,
        gate::{gate, landing_url},
        write::commit_write,
    },
    bulk::{MAX_BULK_IDS, parse_bulk_ids},
    fetch::{composite_pk_error, find_by_key},
};
use crate::{
    db::db,
    notification::{Notification, set_notification},
    policy::Ability,
    resource::{ActionEntry, Committed, Mounted, Resource},
    topcoat_compat::async_page,
};

/// Failure-toast wording for a custom action.
const WRITE_ACTION: &str = "run the action";

/// Which records a custom action POST names.
#[derive(Clone, Copy)]
enum Target {
    /// The one record in the URL: `{list}/{key}/actions/{name}`.
    Row,
    /// The `ids` the bulk form carried: `{list}/actions/{name}`.
    Bulk,
}

/// A row's custom action POST: `{list}/{key}/actions/{name}`.
pub(crate) fn resource_row_action<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    run_action::<R>(cx, body, Target::Row)
}

/// The bulk bar's custom action POST: `{list}/actions/{name}`, with the
/// selection in the `ids` field.
pub(crate) fn resource_bulk_action<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    run_action::<R>(cx, body, Target::Bulk)
}

/// Runs the custom action both routes share.
fn run_action<R: Resource>(cx: &Cx, body: Body, target: Target) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can(cx, Ability::ViewAny) {
            return Err(forbidden().into());
        }
        let name = topcoat::router::path_param_segment(cx, "action").to_string();
        let Some(action) = resource.actions.find(&name).filter(|action| match target {
            Target::Row => action.row,
            Target::Bulk => action.bulk,
        }) else {
            return Err(not_found().into());
        };
        let values = parse_form_body(cx, body).await?.values;
        crate::csrf::verify(cx, &values)?;
        let ids = match target {
            Target::Row => {
                vec![topcoat::router::path_param_segment(cx, "id").to_string()]
            }
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
        let mut db = db(cx);
        let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
        let rows = load_targets(cx, &resource, &ids, target, &mut tx).await?;
        if rows.iter().any(|row| !resource.can(cx, Ability::View(row))) {
            return Err(forbidden().into());
        }
        let refused = rows.iter().filter(|row| !(action.can_run)(cx, row)).count();
        if refused > 0 {
            return Err(refuse(cx, &resource, action, target, refused));
        }
        let count = rows.len();
        let written = (action.run)(cx, &rows, &mut tx).await.map(|()| rows);
        let name = action.name;
        commit_write(
            cx,
            &resource,
            tx,
            written,
            |rows| Committed::acted(name, rows),
            (action.success)(cx, count),
            WRITE_ACTION,
        )
        .await
    })
}

/// Loads the records `ids` names through the tenant-scoped query inside the transaction.
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

/// Answers a target holding `refused` records the action may not run on.
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
