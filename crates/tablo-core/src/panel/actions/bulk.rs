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
        forms::{commit_write, parse_form_body, truthy},
        gate::{gate, list_url},
    },
    fetch::composite_pk_error,
};
use crate::{
    db::db,
    notification::{Notification, set_notification},
    resource::{Committed, Resource},
};

/// Failure-toast wording for the delete handlers.
const WRITE_BULK_DELETE: &str = "delete the selected rows";

/// Bulk delete POST — ids via `ids` form field (comma-separated).
///
/// Identity is the typed PK fetch alone: the display key
/// is never re-matched, so non-canonical keys (uppercase UUID,
/// email key) cannot 404 a batch whose rows exist. Bounded by
/// `MAX_BULK_IDS` so the `IN` list cannot be amplified into a DoS.
/// Fetch, policy checks, and deletes share one framework transaction
/// a mid-loop failure deletes zero rows.
pub(crate) fn resource_bulk_delete<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::<_, BoxView<'_>>::new(
        async move {
            gate::<R>(cx)?;
            // The whole-resource half of the policy, before the body is read:
            // a resource that allows no delete renders no delete chrome.
            if !R::can_delete_any(cx) {
                return Err(forbidden().into());
            }
            // Delete/bulk-delete carry no file parts: only the values half is read.
            let values = parse_form_body(cx, body).await?.values;
            crate::csrf::verify(cx, &values)?;
            // Confirmation marker, mirroring the row delete: the bulk
            // bar's dialog carries `confirm=1`, so a POST without it did not
            // come from the confirming control. Checked after CSRF verification
            // and before any DB work — a forged POST answers 400
            // without touching a connection.
            if !values.get("confirm").is_some_and(|v| truthy(v)) {
                return Err(topcoat::router::error::bad_request(
                    "bulk delete requires confirmation",
                )
                .into());
            }
            let ids_raw = values.get("ids").cloned().unwrap_or_default();
            let ids = parse_bulk_ids(&ids_raw, MAX_BULK_IDS);
            if ids.is_empty() {
                // No ids is a validation miss, not a raw 400 page:
                // the bulk bar cannot submit without a selection, so only a
                // crafted (or stale) POST gets here — answer like any other
                // mutation, with the list and the reason.
                set_notification(cx, Notification::error("Select at least one row to delete"));
                return Err(see_other(list_url(cx, &R::slug())).into());
            }
            if ids.len() > MAX_BULK_IDS {
                return Err(topcoat::router::error::bad_request(format!(
                    "too many ids (max {MAX_BULK_IDS})"
                ))
                .into());
            }
            // Fetch only the requested rows through the tenancy-scoped seam:
            // one `pk IN (…)` query replaces the #75 item-1
            // fetch-everything-then-match loop. A malformed id cannot exist and
            // maps to 404; a missing/wrong-tenant id makes the fetch come back
            // short and 404s as well.
            let keys: Vec<&str> = ids.iter().map(String::as_str).collect();
            let Some(pk_filter) = crate::schema::pk_in_expr::<R::Model>(&keys) else {
                if let Some(error) = composite_pk_error::<R>() {
                    return Err(error);
                }
                return Err(topcoat::router::error::not_found().into());
            };
            let mut db = db(cx);
            let mut tx = db.transaction().await.map_err(crate::db::unavailable)?;
            // The batch fetch reads only the records' own columns:
            // the policy predicates and the write below never touch a relation.
            let rows = crate::resource::scoped_query::<R>(cx)?
                .filter(pk_filter)
                .exec(&mut tx)
                .await
                .map_err(crate::db::unavailable)?;
            if rows.len() != ids.len() {
                return Err(topcoat::router::error::not_found().into());
            }
            for rec in &rows {
                // Edit contract on every row: viewing precedes
                // deleting, same as the edit GET/POST pair.
                if !R::can_view(cx, rec) {
                    return Err(forbidden().into());
                }
                if !R::can_delete(cx, rec) {
                    return Err(forbidden().into());
                }
            }
            // All checks passed — perform bulk delete inside the tx, then
            // commit once. Any error drops `tx` uncommitted: zero rows
            // deleted, never half-applied. The hook names the whole batch: a
            // bulk delete is one write, so it is one `after_commit` call.
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

/// Max ids accepted by bulk delete: bounds the `IN` list.
const MAX_BULK_IDS: usize = 400;

/// Parse + dedupe bulk `ids` while preserving order, so a repeated id can't
/// make the fetched-rows count check misfire.
///
/// `max` bounds the parse itself, not just the final list: a 10 MiB
/// body of distinct ids stops at `max + 1` entries (which the handler then
/// rejects with 400) instead of allocating millions of strings while the
/// `MAX_BULK_IDS` check waits for the parse to finish. Deduping uses a set, so
/// the scan stays linear in the number of ids.
///
/// Known limit: the split happens after url-decoding, so a
/// `String`-PK id containing a literal comma (`%2C`) splits into phantom
/// ids and the batch 404s. Comma-bearing string PKs need a different
/// transport (future work); all other PK types are comma-free.
fn parse_bulk_ids(raw: &str, max: usize) -> Vec<String> {
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
