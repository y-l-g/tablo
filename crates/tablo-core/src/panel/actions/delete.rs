//! Delete action POST — confirmation-marked, policy-checked, run in the
//! framework transaction: the checked record flows into the write.

use topcoat::{
    context::Cx,
    router::{Body, error::forbidden},
    view::{BoxView, HoistView, internal::ThenView},
};

use super::{
    super::{
        forms::{parse_form_body, truthy},
        gate::gate,
        write::commit_write,
    },
    fetch::find_by_key,
};
use crate::{
    db::db,
    policy::{Ability, can},
    resource::{Committed, Resource},
};

/// Failure-toast wording for the delete handlers.
const WRITE_DELETE: &str = "delete the record";

/// Delete action POST — confirmation-marked, policy-checked, and run in the
/// framework transaction: the checked record flows into the write.
///
/// The confirmation is the row's alert dialog on the list page: the
/// Delete link opens `?delete=<key>` and the dialog's form POSTs here with
/// `confirm=1`. Authentication comes before any DB work: the CSRF
/// check and the confirmation marker run first, so a forged POST answers 403
/// without opening a transaction, holding a pooled connection across the body
/// read, or probing record existence (create/bulk-delete ordering).
/// The dialog itself is deliberately fetch-free and policy-blind: it carries
/// no record data and embeds only the caller's own CSRF token, and the
/// policy/tenancy checks run against the loaded record here.
pub(crate) fn resource_delete<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::<_, BoxView<'_>>::new(
        async move {
            gate::<R>(cx)?;
            // The whole-resource half of the policy, before the body is read:
            // a resource that allows no delete renders no delete chrome.
            if !can::<R>(cx, Ability::DeleteAny) {
                return Err(forbidden().into());
            }
            // Delete/bulk-delete carry no file parts: only the values half is read.
            let values = parse_form_body(cx, body).await?.values;
            crate::csrf::verify(cx, &values)?;
            let confirmed = values.get("confirm").is_some_and(|v| truthy(v));
            if !confirmed {
                // The confirmation UI is the list-page alert dialog:
                // the row link opens `?delete=<key>` and the dialog's form carries
                // `confirm=1`. This route only accepts that confirmed POST, so a
                // missing marker is a malformed client, not a user path.
                return Err(
                    topcoat::router::error::bad_request("delete requires confirmation").into(),
                );
            }
            // Confirmed and authenticated: open the transaction only now (GH
            // #144), fetch through the tenant-scoped query, check Policy against the
            // loaded record, and delete inside the tx — commit makes the checked
            // delete durable, any error rolls it back. Delete takes
            // the edit contract: `View` plus
            // `Delete` — a record that cannot be viewed cannot be deleted
            // by UUID-guessing the route.
            let mut db = db(cx);
            let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
            let id = topcoat::router::path_param_segment(cx, "id").to_string();
            // The delete path reads only the record's own columns:
            // `View`/`Delete` are Rust predicates over those, and
            // `delete_record` reads the same snapshot.
            let record = find_by_key::<R>(cx, &id, &mut tx).await?;
            if !can::<R>(cx, Ability::View(&record)) {
                return Err(forbidden().into());
            }
            if !can::<R>(cx, Ability::Delete(&record)) {
                return Err(forbidden().into());
            }
            // The hook names what was removed: the pre-delete snapshot, since
            // the row is gone by the time it runs.
            let written = R::delete_record(cx, &record, &mut tx)
                .await
                .map(|()| vec![record]);
            commit_write::<R, _>(cx, tx, written, Committed::deleted, "Deleted", WRITE_DELETE).await
        },
    )))
}

#[cfg(test)]
mod tests;
