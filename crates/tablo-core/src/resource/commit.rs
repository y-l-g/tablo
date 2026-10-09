//! What a mutation committed, and the one place the framework says so.

use topcoat::context::Cx;

use super::{Action, Resource};

/// The kind of mutation a record fn performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Mutation {
    Create,
    Update,
    Delete,
    /// The custom [`Action`](super::Action) or [`HeaderAction`](crate::HeaderAction) of this
    /// `NAME`. A header action's [`Committed`] holds no record.
    Action(&'static str),
    /// A many-to-many relation's table linked a record to the one [`Committed`] holds.
    Attach,
    /// A many-to-many relation's table unlinked records from the one [`Committed`] holds.
    Detach,
}

/// What one committed mutation wrote.
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

    /// The row an update wrote, as it stands after the write.
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

    /// The rows a custom [`Action`](super::Action) `A` ran on.
    pub fn acted<R: Resource<Model = M>, A: Action<R>>(records: Vec<M>) -> Self {
        Self {
            mutation: Mutation::Action(A::NAME),
            records,
        }
    }

    /// A relation's table linked a record to `owner`.
    pub(crate) fn attached(owner: M) -> Self {
        Self {
            mutation: Mutation::Attach,
            records: vec![owner],
        }
    }

    /// A relation's table unlinked records from `owner`.
    pub(crate) fn detached(owner: M) -> Self {
        Self {
            mutation: Mutation::Detach,
            records: vec![owner],
        }
    }

    /// The header action named `name` ran: it acts on no record.
    pub(crate) fn ran(name: &'static str) -> Self {
        Self {
            mutation: Mutation::Action(name),
            records: Vec::new(),
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
/// A hook returning `Err` is logged and ignored: the write stays committed.
pub(crate) async fn run_after_commit<R: Resource>(cx: &Cx, committed: Committed<R::Model>) {
    if let Err(error) = R::after_commit(cx, committed).await {
        tracing::error!(
            error = %error,
            resource = std::any::type_name::<R>(),
            "after_commit failed; the write stays committed"
        );
    }
}
