//! Bulk delete POST — ids via the `ids` form field, bounded, confirmed,
//! all-or-nothing in one framework transaction.

use topcoat::{
    context::Cx,
    router::{
        Body,
        error::{forbidden, see_other},
    },
    view::{BoxView, HoistView, internal::ThenView},
};

use super::{
    super::{
        forms::{parse_form_body, truthy},
        gate::{gate, landing_url},
        write::commit_write,
    },
    fetch::composite_pk_error,
};
use crate::{
    db::db,
    notification::{Notification, set_notification},
    policy::{Ability, can},
    resource::{Committed, Resource},
};

/// Failure-toast wording for the delete handlers.
const WRITE_BULK_DELETE: &str = "delete the selected rows";

/// Serves the bulk delete POST over the `ids` form field.
pub(crate) fn resource_bulk_delete<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::<_, BoxView<'_>>::new(
        async move {
            gate::<R>(cx)?;
            if !can::<R>(cx, Ability::DeleteAny) {
                return Err(forbidden().into());
            }
            let values = parse_form_body(cx, body).await?.values;
            crate::csrf::verify(cx, &values)?;
            // Requires the `confirm=1` marker.
            if !values.get("confirm").is_some_and(|v| truthy(v)) {
                return Err(topcoat::router::error::bad_request(
                    "bulk delete requires confirmation",
                )
                .into());
            }
            let ids_raw = values.get("ids").cloned().unwrap_or_default();
            let ids = parse_bulk_ids(&ids_raw, MAX_BULK_IDS);
            if ids.is_empty() {
                set_notification(cx, Notification::error("Select at least one row to delete"));
                return Err(see_other(landing_url(cx, &R::slug())).into());
            }
            if ids.len() > MAX_BULK_IDS {
                return Err(topcoat::router::error::bad_request(format!(
                    "too many ids (max {MAX_BULK_IDS})"
                ))
                .into());
            }
            // Fetches only the requested rows through the tenancy-scoped query.
            let keys: Vec<&str> = ids.iter().map(String::as_str).collect();
            let Some(pk_filter) = crate::toasty_compat::pk::pk_in_expr::<R::Model>(&keys) else {
                if let Some(error) = composite_pk_error::<R>() {
                    return Err(error);
                }
                return Err(topcoat::router::error::not_found().into());
            };
            let mut db = db(cx);
            let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
            let rows = crate::resource::scoped_query::<R>(cx)?
                .filter(pk_filter)
                .exec(&mut tx)
                .await
                .map_err(crate::error::unavailable)?;
            if rows.len() != ids.len() {
                return Err(topcoat::router::error::not_found().into());
            }
            // Viewing precedes deleting on every row.
            if rows.iter().any(|rec| !can::<R>(cx, Ability::View(rec))) {
                return Err(forbidden().into());
            }
            let refused = rows
                .iter()
                .filter(|rec| !can::<R>(cx, Ability::Delete(rec)))
                .count();
            if refused > 0 {
                let noun = if refused == 1 { "record" } else { "records" };
                set_notification(
                    cx,
                    Notification::error(format!(
                        "{refused} selected {noun} cannot be deleted; nothing was deleted"
                    )),
                );
                return Err(see_other(landing_url(cx, &R::slug())).into());
            }
            let written = R::bulk_delete_records(cx, &rows, &mut tx)
                .await
                .map(|()| rows);
            commit_write::<R, _>(
                cx,
                tx,
                written,
                Committed::deleted,
                "Bulk deleted",
                WRITE_BULK_DELETE,
            )
            .await
        },
    )))
}

/// Bounds the bulk delete `IN` list.
pub(super) const MAX_BULK_IDS: usize = 400;

/// Parses and dedupes bulk `ids` while preserving order.
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
