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

#[cfg(test)]
mod tests;
