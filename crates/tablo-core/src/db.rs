//! The `Db` glue: how Tablo code reaches the pooled Toasty database.
//!
//! `Db` is registered once on the app context (`app_context::<Db>`), and each
//! request clones it — a cheap `Arc` bump — before running Toasty statements,
//! which require `&mut Db`.

use toasty::Db;
use topcoat::context::{Cx, app_context};

/// Returns the pooled [`Db`] registered on the app context.
///
/// Cloning the `Db` is cheap; Toasty statements need `&mut Db`, so callers do
/// `let mut db = db(cx); ...exec(&mut db).await?`.
///
/// Pool discipline: while a framework transaction holds its
/// connection, no other handle may run statements — with a single-connection
/// pool (notably `sqlite::memory:`) a second handle blocks forever. Mutation
/// handlers therefore keep the tx strictly around check + write + commit and
/// `drop(tx)` before any re-render (form option loaders open their own
/// handle).
///
/// List pages stream: the table query runs while the response body streams,
/// so a live list body holds a pooled connection until drained or dropped.
/// With a single-connection pool a second query waits while an earlier list
/// body stays alive; drain or drop the body before the next query.
#[inline]
pub fn db(cx: &Cx) -> Db {
    app_context::<Db>(cx).clone()
}

/// Map a database infrastructure failure (pool/tx open, probe/exec, commit)
/// to an opaque 500: the driver/SQL text is logged for operators
/// but never reaches the error page. The streamed list already holds this
/// contract through its generic `ErrorState` (logged once at
/// `table_error_view`); mutation/export paths must match it.
///
/// Only infra failures come here. App-hook errors (`create_record` et al.)
/// and explicit guards (404s, 403s, config errors) keep their own mapping;
/// [`hook_failure`] is the seam that separates the two when both arrive
/// through the same record fn.
pub(crate) fn unavailable(source: impl std::fmt::Display) -> topcoat::Error {
    tracing::error!(error = %source, "database unavailable");
    topcoat::Error::from(std::io::Error::other("database unavailable"))
}

/// Map a failed record hook to the error the response carries.
///
/// A hook (`create_record`, `update_record`, `delete_record`, …) is app code,
/// so its error is one of two things: the app's own — a guard's 404, a config
/// error, the default stub's "not implemented" — or the driver's.
/// keeps the app's mapping (a guard must still answer 404, not a 500), while
/// the driver's is an infra failure and takes [`unavailable`]: its text goes
/// to the log, and the page never echoes it. The error's own type settles
/// which one it is, never its message — Toasty owns [`toasty::Error`], so a
/// downcast decides (the same seam the crate uses for
/// `ContentTooLargeError` and `CursorDecodeError`).
pub(crate) fn hook_failure(error: topcoat::Error) -> topcoat::Error {
    if error.is::<toasty::Error>() {
        unavailable(error)
    } else {
        error
    }
}

#[cfg(test)]
mod tests;
