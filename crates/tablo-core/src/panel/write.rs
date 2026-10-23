//! The write tail every mutation shares: create, update, the deletes, the custom actions and
//! the relation links end in [`commit_write`], and the header actions in [`commit_then`].

use std::future::Future;

use topcoat::{context::Cx, router::error::see_other, view::BoxView};

use super::{gate::landing_url, state::uploader};
use crate::{
    db::WriteTx,
    flash::{notify_write_failure, set_notification},
    form::RecordForm,
    notification::Notification,
    outcome::{Outcome, same_origin_path},
    resource::{Change, Mounted, Resource},
};

/// Run the before-commit hook on what the record fn wrote, commit the transaction, run the
/// after-commit hook, and redirect to the list with the success flash. A failed write, hook or
/// commit maps to the caller's failure toast and an opaque error.
///
/// `written` is the record fn's result, carrying what the hooks receive and the [`Outcome`] the
/// response follows; `change` names what it changed, `note` the default success flash, and
/// `failure` the toast.
pub(crate) async fn commit_write<'a, R: Resource, T>(
    cx: &'a Cx,
    resource: &Mounted<R>,
    mut tx: WriteTx<'_>,
    written: Result<(T, Outcome), topcoat::Error>,
    change: impl FnOnce(T) -> Change<R::Model>,
    note: impl Into<String>,
    failure: &'static str,
) -> Result<BoxView<'a>, topcoat::Error> {
    let written = match written {
        Ok((value, outcome)) => {
            let change = change(value);
            match R::before_commit(cx, &change, &mut *tx).await {
                Ok(()) => match end_changed_credentials(cx, &mut *tx, &change).await {
                    Ok(()) => keep_uploads(cx, resource, &change)
                        .await
                        .map(|()| (change, outcome)),
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    };
    commit_then(
        cx,
        tx,
        written,
        |change| async move {
            discard_uploads(cx, resource, &change).await;
            crate::resource::run_after_commit::<R>(cx, change).await;
        },
        note,
        failure,
        &resource.url,
    )
    .await
}

/// Ends the sessions of the admin `change` gave a new email or password, inside the write's
/// transaction, so a committed credential change leaves no session alive.
async fn end_changed_credentials<M: Send + Sync + 'static>(
    cx: &Cx,
    tx: &mut dyn toasty::Executor,
    change: &Change<M>,
) -> topcoat::Result<()> {
    match change {
        Change::Update { before, after } => {
            crate::auth::end_changed_credentials(cx, tx, before, after).await
        }
        _ => Ok(()),
    }
}

/// Keeps the new uploads `change` stores, before it commits.
async fn keep_uploads<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    change: &Change<R::Model>,
) -> Result<(), topcoat::Error> {
    let hydrate = |record| <R::Form as RecordForm>::hydrate(cx, record);
    let (before, after) = match change {
        Change::Create(record) => (None, hydrate(record)),
        Change::Update { before, after } => (Some(hydrate(before)), hydrate(after)),
        _ => return Ok(()),
    };
    crate::upload::keep(uploader(cx), &resource.form, before.as_ref(), &after)
        .await
        .map_err(crate::error::unavailable)
}

/// Deletes the uploads a committed `change` let go: every file of a removed record, and a file
/// an update cleared or replaced. A soft delete keeps them for the restore.
async fn discard_uploads<R: Resource>(cx: &Cx, resource: &Mounted<R>, change: &Change<R::Model>) {
    let hydrate = |record| <R::Form as RecordForm>::hydrate(cx, record);
    let (before, after) = match change {
        Change::Update { before, after } => (vec![hydrate(before)], Some(hydrate(after))),
        Change::Delete(records) if resource.soft_delete.is_none() => {
            (records.iter().map(hydrate).collect(), None)
        }
        Change::ForceDelete(records) => (records.iter().map(hydrate).collect(), None),
        _ => return,
    };
    crate::upload::discard(uploader(cx), &resource.form, &before, after.as_ref()).await;
}

/// [`commit_write`]'s tail for any write: commit, run `after` on what was written, and land as
/// the write's [`Outcome`] asks: by default on `home` or the request's `?return=` with the
/// success flash.
pub(crate) async fn commit_then<'a, T, F: Future<Output = ()>>(
    cx: &'a Cx,
    tx: WriteTx<'_>,
    written: Result<(T, Outcome), topcoat::Error>,
    after: impl FnOnce(T) -> F,
    note: impl Into<String>,
    failure: &'static str,
    home: &str,
) -> Result<BoxView<'a>, topcoat::Error> {
    match written {
        Ok((value, outcome)) => match tx.commit().await {
            Ok(()) => {
                // Post-commit, so the effect cannot survive a rollback, and
                // the tx is gone, so the hook may open its own handle.
                after(value).await;
                Err(redirect_after_write(cx, home, note.into(), outcome))
            }
            Err(error) => {
                notify_write_failure(cx, failure);
                Err(crate::error::unavailable(error))
            }
        },
        Err(error) => Err(failed_write(cx, failure, error)),
    }
}

/// Flashes `failure` and maps a record fn's `error` to the response.
///
/// The error is not echoed raw: the driver's text goes to the log through the opaque mapping,
/// and an app-authored error keeps its own. The create and edit forms first re-probe their unique
/// fields, so a value a concurrent write took renders inline instead (tablo#117).
pub(crate) fn failed_write(
    cx: &Cx,
    failure: &'static str,
    error: topcoat::Error,
) -> topcoat::Error {
    notify_write_failure(cx, failure);
    crate::error::driver_failure(error, crate::error::DATABASE_UNAVAILABLE)
}

/// Post/Redirect/Get with a flash notification, to `home` or the page the
/// write's `?return=` names ([`landing_url`]), unless `outcome` names its own
/// notification or landing. The browser follows with a GET, and the flash cookie
/// rides the error response (Topcoat flushes `Set-Cookie` on `Err` too,
/// topcoat#408), so every mutation redirects the same way.
fn redirect_after_write(cx: &Cx, home: &str, note: String, outcome: Outcome) -> topcoat::Error {
    let notification = outcome
        .notification()
        .cloned()
        .unwrap_or_else(|| Notification::success(note));
    set_notification(cx, notification);
    let target = match outcome.redirect_path() {
        Some(path) => match same_origin_path(path) {
            Some(path) => path.to_string(),
            None => {
                // The write has committed: land where the run started rather than fail it.
                tracing::error!(path, "an action's outcome redirects off the site; ignored");
                landing_url(cx, home)
            }
        },
        None => landing_url(cx, home),
    };
    see_other(target).into()
}
