//! The `Db` glue: how Tablo code reaches the pooled Toasty database, and the transaction a panel
//! write runs its record fns and hooks in.

use std::{
    ops::{Deref, DerefMut},
    sync::{Mutex, PoisonError},
};

use toasty::{Db, Transaction};
use topcoat::context::{Cx, CxId, context};

/// Returns the pooled [`Db`] registered on the router.
///
/// Never runs statements on another handle while a transaction holds its connection; drains or
/// drops streaming list bodies before the next query.
///
/// # Errors
///
/// A declaration failure when called while a panel write's transaction is open for the same
/// request, such as from [`Resource::before_commit`](crate::Resource::before_commit) or a record
/// fn: run the statement through the `ex` the hook received instead.
#[inline]
pub fn db(cx: &Cx) -> topcoat::Result<Db> {
    no_write_open(cx.id(), "db(cx)")?;
    Ok(context::<Db>(cx).clone())
}

/// How many rows `query` holds, counted on the pooled [`Db`].
///
/// Reads 0 where Toasty answers no value: a count over a filter Toasty can tell matches nothing,
/// such as [`list_query`](crate::list_query) narrowed to one status on a list whose tab shows
/// another, which `Query::count` fails to read.
///
/// # Errors
///
/// As [`db`], and the database failing the count.
pub async fn count<M: toasty::schema::Model>(
    cx: &Cx,
    query: toasty::stmt::Query<toasty::stmt::List<M>>,
) -> topcoat::Result<u64> {
    crate::toasty_compat::count(query, &mut db(cx)?)
        .await
        .map_err(crate::error::unavailable)
}

/// The requests with a panel write's transaction open now, each with the work to run once it
/// commits. A request opens one at a time, and few run at once, so a list is enough.
static OPEN: Mutex<Vec<Open>> = Mutex::new(Vec::new());

/// One request's open transaction.
struct Open {
    request: CxId,
    after_commit: Vec<Box<dyn FnOnce() + Send>>,
}

fn open() -> std::sync::MutexGuard<'static, Vec<Open>> {
    OPEN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Refuses when `request` has a panel write's transaction open: a second connection taken now
/// writes outside it, and on a pool of one waits forever for the connection the transaction
/// holds.
pub(crate) fn no_write_open(request: CxId, what: &str) -> topcoat::Result<()> {
    if open().iter().any(|open| open.request == request) {
        return Err(crate::error::declaration(format!(
            "{what} opens a second connection while this request's write transaction is open: \
             its statements would run outside the transaction, and on a pool of one it waits \
             forever. Run them through the `ex` the record fn or hook received."
        )));
    }
    Ok(())
}

/// Queues `work` to run once `request`'s open panel write commits, and drops it if the write
/// rolls back. Hands `work` back when no panel write is open for `request`.
pub(crate) fn after_write_commits<F: FnOnce() + Send + 'static>(
    request: CxId,
    work: F,
) -> Result<(), F> {
    match open().iter_mut().find(|open| open.request == request) {
        Some(open) => {
            open.after_commit.push(Box::new(work));
            Ok(())
        }
        None => Err(work),
    }
}

/// The transaction a panel write runs in. While it is open, [`db`] refuses the same request,
/// and [`Notifications::send_in`](crate::Notifications::send_in)
/// wakes bells only once it commits.
pub(crate) struct WriteTx<'a> {
    /// `None` only inside [`Self::commit`], once the transaction is consumed.
    tx: Option<Transaction<'a>>,
    request: CxId,
}

impl<'a> WriteTx<'a> {
    /// Opens a transaction on `db` for `cx`'s request.
    pub(crate) async fn begin(cx: &Cx, db: &'a mut Db) -> toasty::Result<Self> {
        let tx = db.transaction().await?;
        let request = cx.id();
        let mut open = open();
        debug_assert!(
            open.iter().all(|open| open.request != request),
            "a request opens one write transaction at a time"
        );
        open.push(Open {
            request,
            after_commit: Vec::new(),
        });
        Ok(Self {
            tx: Some(tx),
            request,
        })
    }

    /// Commits, then runs what [`after_write_commits`] queued.
    pub(crate) async fn commit(mut self) -> toasty::Result<()> {
        let committed = match self.tx.take() {
            Some(tx) => tx.commit().await,
            None => Ok(()),
        };
        let after_commit = self.close();
        if committed.is_ok() {
            for work in after_commit {
                work();
            }
        }
        committed
    }

    /// Unregisters this transaction, answering the work queued for its commit.
    fn close(&mut self) -> Vec<Box<dyn FnOnce() + Send>> {
        let mut open = open();
        match open.iter().position(|open| open.request == self.request) {
            Some(index) => open.swap_remove(index).after_commit,
            None => Vec::new(),
        }
    }
}

impl<'a> Deref for WriteTx<'a> {
    type Target = Transaction<'a>;

    fn deref(&self) -> &Self::Target {
        self.tx.as_ref().expect("open until commit")
    }
}

impl DerefMut for WriteTx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.tx.as_mut().expect("open until commit")
    }
}

impl Drop for WriteTx<'_> {
    /// A transaction dropped uncommitted rolls back, and what waited for its commit with it.
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests;
