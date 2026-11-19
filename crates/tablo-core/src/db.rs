//! The `Db` glue: how Tablo code reaches the pooled Toasty database, and the transaction a panel
//! write runs its record fns and hooks in.

use std::{
    ops::{Deref, DerefMut},
    sync::{Mutex, PoisonError},
};

use toasty::{
    Db, Transaction,
    stmt::{IntoStatement, List},
};
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

/// How many rows `query` holds, counted on the pooled [`Db`]: a model's query, such as
/// `Post::filter(..)`, or a [`Query`](toasty::stmt::Query) such as
/// [`list_query`](crate::list_query)'s.
///
/// Reads 0 where Toasty answers no value: a count over a filter Toasty can tell matches nothing,
/// such as [`list_query`](crate::list_query) narrowed to one status on a list whose tab shows
/// another, which `Query::count` fails to read.
///
/// A record fn or a hook counts through the `ex` it received, with [`count_in`].
///
/// # Errors
///
/// As [`db`], and the database failing the count.
pub async fn count<M: toasty::schema::Model>(
    cx: &Cx,
    query: impl IntoStatement<Returning = List<M>>,
) -> topcoat::Result<u64> {
    count_in(&mut db(cx)?, query).await
}

/// How many rows `query` holds, counted through `ex`: inside the transaction a record fn or a
/// hook received, so the count sees the rows the write has made so far and a limit check holds
/// against concurrent writes the isolation level orders. Reads 0 where Toasty answers no value, as
/// [`count`] does.
///
/// ```rust,no_run
/// # use tablo_core::{NoForm, Resource, WriteError, db};
/// # use topcoat::context::Cx;
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
/// # struct Project { #[key] #[auto] id: uuid::Uuid, team_id: String }
/// # struct ProjectResource;
/// # impl Resource for ProjectResource { type Model = Project; type Form = NoForm<Project>; }
/// # async fn check(cx: &Cx, team: String, ex: &mut dyn toasty::Executor) -> topcoat::Result<bool> {
/// let projects = db::count_in(ex, Project::filter(Project::fields().team_id().eq(team))).await?;
/// Ok(projects < 10)
/// # }
/// ```
///
/// # Errors
///
/// The database failing the count, and a declaration failure for a statement that writes, such as
/// an insert, which has no rows to count.
pub async fn count_in<M: toasty::schema::Model>(
    ex: &mut dyn toasty::Executor,
    query: impl IntoStatement<Returning = List<M>>,
) -> topcoat::Result<u64> {
    let Some(query) = query.into_statement().into_query() else {
        return Err(crate::error::declaration(
            "db::count counts the rows a query reads, not a statement that writes",
        ));
    };
    crate::toasty_compat::count(query, ex)
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
