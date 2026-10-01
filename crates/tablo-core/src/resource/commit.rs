//! What a mutation committed, and the one place the framework says so.
//!
//! The write handlers own the transaction: a `Resource` record fn
//! writes through `&mut dyn toasty::Executor` and the framework commits. That
//! leaves nowhere correct for a side effect that must *not* happen on a
//! rollback — an email, a webhook, an audit row, cache invalidation: doing it
//! inside the record fn leaks it when the transaction rolls back, and opening a
//! second handle while the transaction holds the pool is the discipline
//! problem the handlers exist to avoid.
//!
//! So the framework reports the commit instead: [`Committed`] names the
//! mutation and the rows it wrote, [`Resource::after_commit`] receives it once
//! per successful write, and the transaction is gone by then.

use topcoat::context::Cx;

use super::Resource;

/// The kind of mutation a record fn performed.
///
/// The vocabulary `Action` names in `CONTEXT.md`, as a value: the framework
/// knows which record fn or [`Action`](super::Action) ran, so an audit row or
/// a webhook payload does not have to be spelled per call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Mutation {
    Create,
    Update,
    Delete,
    /// The custom action of this [`NAME`](super::Action::NAME).
    Action(&'static str),
}

/// What one committed mutation wrote, handed to
/// [`Resource::after_commit`](super::Resource::after_commit).
///
/// The handlers build one on every successful write: `created` carries the row
/// a create returned, `updated` the row an update returned (the committed
/// state), `deleted` the rows a delete or bulk delete removed — one value per
/// write, so a bulk delete is *one* `Committed` however many rows it took. The
/// constructors are public for the other direction: an app exercising its own
/// `after_commit` in a test builds the value it wants to hand it.
#[derive(Debug, Clone)]
pub struct Committed<M> {
    mutation: Mutation,
    records: Vec<M>,
}

impl<M> Committed<M> {
    /// The row a create wrote.
    pub fn created(record: M) -> Self {
        Self {
            mutation: Mutation::Create,
            records: vec![record],
        }
    }

    /// The row an update wrote, as it stands after the write — what
    /// `update_record` returned, not the snapshot the handler loaded.
    pub fn updated(record: M) -> Self {
        Self {
            mutation: Mutation::Update,
            records: vec![record],
        }
    }

    /// The rows a delete removed, as they were before the delete.
    pub fn deleted(records: Vec<M>) -> Self {
        Self {
            mutation: Mutation::Delete,
            records,
        }
    }

    /// The rows a custom [`Action`](super::Action) named `name` ran on, as
    /// they were loaded before it ran.
    pub fn acted(name: &'static str, records: Vec<M>) -> Self {
        Self {
            mutation: Mutation::Action(name),
            records,
        }
    }

    /// Which record fn or custom action ran.
    pub fn mutation(&self) -> Mutation {
        self.mutation
    }

    /// The rows the mutation wrote, in the order the handler had them.
    pub fn records(&self) -> &[M] {
        &self.records
    }
}

/// Deliver a committed mutation to the app.
///
/// The framework's single call site, so the failure policy cannot drift
/// between the write handlers, custom actions included: a hook that returns `Err` is **logged and
/// ignored**. The write is committed — the row is in the database, the
/// response is the redirect the user earned — so turning a failed email into
/// an error page would misreport what happened, and rolling back is not
/// available. Retries and delivery guarantees are deliberately not the
/// framework's promise; an app that needs them writes its own outbox here.
///
/// A hook that *panics* is not caught here: Topcoat isolates a panicking
/// request into a 500, which is loud and still leaves the write committed. That
/// is a bug in the hook, not a reported failure, and it is why this only
/// handles `Err`.
pub(crate) async fn run_after_commit<R: Resource>(cx: &Cx, committed: Committed<R::Model>) {
    if let Err(error) = R::after_commit(cx, committed).await {
        tracing::error!(
            error = %error,
            resource = R::slug(),
            "after_commit failed; the write stays committed"
        );
    }
}

#[cfg(test)]
mod tests;
