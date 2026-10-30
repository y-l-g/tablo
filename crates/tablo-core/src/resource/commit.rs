//! What a write changed, and the one place the framework hands it to the app's hooks.

use topcoat::context::Cx;

use super::{Action, HeaderAction, Resource};

/// What one write of a resource changed: the mutation and the rows it wrote.
///
/// [`Resource::before_commit`] receives it inside the write's transaction and
/// [`Resource::after_commit`] once the transaction commits.
///
/// The enum is exhaustive on purpose: an audit hook that matches every variant stops compiling
/// when a new kind of write appears, instead of skipping it.
///
/// A create or an update that also links or unlinks records through a many-to-many form field
/// reports the record only: the links are not in `M`. Read them through the hook's executor when
/// an audit needs them; the relation table's own writes report as [`Change::Attach`] and
/// [`Change::Detach`].
///
/// A save that also writes a [`Relation::has_one`](crate::Relation::has_one)'s child reports the
/// owner here, and the child to the related resource's hooks, as its own pages would.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change<M> {
    /// A create wrote this row.
    Create(M),
    /// An update wrote `after` over `before`, the snapshot the handler loaded and authorized
    /// inside the transaction. A submission that names no field writes nothing and still
    /// reports an update, with `before == after`.
    Update { before: M, after: M },
    /// A delete or a bulk delete removed these rows, or trashed them on a soft-deleting resource,
    /// as they stood before the delete.
    Delete(Vec<M>),
    /// A restore brought back these trashed rows, as they stood before it: their deleted-at
    /// column still holds the time they were trashed.
    Restore(Vec<M>),
    /// A force delete removed these trashed rows for good, as they stood before it.
    ForceDelete(Vec<M>),
    /// The custom [`Action`] `name` ran on these rows, as the handler loaded them before it ran.
    Action { name: &'static str, records: Vec<M> },
    /// The [`HeaderAction`](crate::HeaderAction) `name` ran, on no record: its audit can record
    /// who ran it, through the context, and which action.
    HeaderAction { name: &'static str },
    /// A many-to-many relation's table linked `owner` to the related record `key` names.
    ///
    /// `relation` is the owner's `#[has_many(via = ..)]` field and `key` the related resource's
    /// record key, as its URLs spell it whatever spelling the request posted. Attaching a linked
    /// record writes nothing and still reports the attach.
    Attach {
        owner: M,
        relation: String,
        key: String,
    },
    /// A many-to-many relation's table unlinked `owner` from the related records `keys` name, as
    /// their URLs spell them.
    Detach {
        owner: M,
        relation: String,
        keys: Vec<String>,
    },
}

impl<M> Change<M> {
    /// The rows of this resource the change names: the created row and the updated row as the
    /// write left them, the deleted, restored or force-deleted rows and the rows an action ran on
    /// as they stood before it, the owner of an attach or a detach, and none for a header action.
    pub fn records(&self) -> &[M] {
        match self {
            Self::Create(record)
            | Self::Update { after: record, .. }
            | Self::Attach { owner: record, .. }
            | Self::Detach { owner: record, .. } => std::slice::from_ref(record),
            Self::Delete(records)
            | Self::Restore(records)
            | Self::ForceDelete(records)
            | Self::Action { records, .. } => records,
            Self::HeaderAction { .. } => &[],
        }
    }

    /// Whether the change is a run of the custom action `A`.
    ///
    /// It compares against `A::NAME`, as [`Ability::is_action`](crate::Ability::is_action) does,
    /// and a type that is not an action of a resource over `M` does not compile. `R` is inferred
    /// when `A` is an action of one resource.
    ///
    /// ```rust
    /// # use tablo_core::{Action, Change, NoForm, Resource};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, published: bool }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// # }
    /// # struct Publish;
    /// # impl Action<PostResource> for Publish {
    /// #     type Input = ();
    /// #     const NAME: &'static str = "publish";
    /// #     async fn run(_: &Cx, _: &[Post], _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError<std::convert::Infallible>> { Ok(tablo_core::Outcome::done()) }
    /// # }
    /// fn published(change: &Change<Post>) -> usize {
    ///     if change.is_action::<Publish, _>() {
    ///         change.records().len()
    ///     } else {
    ///         0
    ///     }
    /// }
    /// ```
    pub fn is_action<A, R>(&self) -> bool
    where
        A: Action<R>,
        R: Resource<Model = M>,
    {
        matches!(self, Self::Action { name, .. } if *name == A::NAME)
    }

    /// Whether the change is a run of the header action `A`.
    ///
    /// It compares against `A::NAME`, as
    /// [`Ability::is_header_action`](crate::Ability::is_header_action) does.
    ///
    /// ```rust
    /// # use tablo_core::{Change, HeaderAction};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid }
    /// # struct RecountTags;
    /// # impl HeaderAction for RecountTags {
    /// #     type Input = ();
    /// #     const NAME: &'static str = "recount-tags";
    /// #     async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError<std::convert::Infallible>> { Ok(tablo_core::Outcome::done()) }
    /// # }
    /// fn audited(change: &Change<Post>) -> bool {
    ///     !change.is_header_action::<RecountTags>()
    /// }
    /// ```
    pub fn is_header_action<A: HeaderAction>(&self) -> bool {
        matches!(self, Self::HeaderAction { name } if *name == A::NAME)
    }
}

/// The change a run of the custom action `A` made to `records`.
pub(crate) fn acted<R: Resource, A: Action<R>>(records: Vec<R::Model>) -> Change<R::Model> {
    Change::Action {
        name: A::NAME,
        records,
    }
}

/// Hand a committed change to the app.
///
/// A hook returning `Err` is logged and ignored: the write stays committed.
pub(crate) async fn run_after_commit<R: Resource>(cx: &Cx, change: Change<R::Model>) {
    if let Err(error) = R::after_commit(cx, change).await {
        tracing::error!(
            error = %error,
            resource = std::any::type_name::<R>(),
            "after_commit failed; the write stays committed"
        );
    }
}
