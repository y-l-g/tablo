//! What a mutation committed, and the one place the framework says so.

use topcoat::context::Cx;

use super::Resource;

/// The kind of mutation a record fn performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Mutation {
    Create,
    Update,
    Delete,
    /// The custom action of this [`NAME`](super::Action::NAME).
    Action(&'static str),
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

    /// The rows a custom [`Action`](super::Action) named `name` ran on.
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

#[cfg(test)]
mod tests;
