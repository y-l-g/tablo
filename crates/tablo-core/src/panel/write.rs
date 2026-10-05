//! The write tail every mutation shares: create, update, delete, and bulk
//! delete end in [`commit_write`].

use topcoat::{context::Cx, router::error::see_other, view::BoxView};

use super::gate::landing_url;
use crate::{
    notification::{Notification, notify_write_failure, set_notification},
    resource::{Committed, Mounted, Resource},
};

/// Commit the transaction, run the after-commit hook on what the record fn
/// wrote, and redirect to the list with the success flash. A failed write or
/// commit maps to the caller's failure toast and an opaque error.
///
/// `written` is the record fn's result, carrying what the hook receives;
/// `committed` names the mutation, `note` the success flash, and `failure`
/// the toast.
pub(crate) async fn commit_write<'a, R: Resource, T>(
    cx: &'a Cx,
    resource: &Mounted<R>,
    tx: toasty::Transaction<'_>,
    written: Result<T, topcoat::Error>,
    committed: impl FnOnce(T) -> Committed<R::Model>,
    note: impl Into<String>,
    failure: &'static str,
) -> Result<BoxView<'a>, topcoat::Error> {
    match written {
        Ok(value) => match tx.commit().await {
            Ok(()) => {
                // Post-commit, so the effect cannot survive a rollback, and
                // the tx is gone, so the hook may open its own handle.
                crate::resource::run_after_commit::<R>(cx, committed(value)).await;
                Err(redirect_after_write(cx, &resource.url, note))
            }
            Err(error) => {
                notify_write_failure(cx, failure);
                Err(crate::error::unavailable(error))
            }
        },
        // A record fn's error is not echoed raw: the driver's text goes to the
        // log through the opaque mapping, and an app-authored error keeps its
        // own. On a create or update this includes a unique violation that
        // slipped past the app-side check (a concurrent write): Toasty exposes
        // no unique-violation predicate (upstream gap #117), so it cannot be
        // classified as an inline field error here.
        Err(error) => {
            notify_write_failure(cx, failure);
            Err(crate::error::driver_failure(
                error,
                crate::error::DATABASE_UNAVAILABLE,
            ))
        }
    }
}

/// Post/Redirect/Get with a flash notification, to the list or the page the
/// write's `?return=` names ([`landing_url`]). The browser follows with a
/// GET, and the flash cookie rides the error response (Topcoat flushes
/// `Set-Cookie` on `Err` too, topcoat#408), so every mutation redirects the
/// same way.
fn redirect_after_write(cx: &Cx, list_url: &str, note: impl Into<String>) -> topcoat::Error {
    set_notification(cx, Notification::success(note));
    see_other(landing_url(cx, list_url)).into()
}
