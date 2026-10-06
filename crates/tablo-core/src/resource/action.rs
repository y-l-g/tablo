//! Custom actions: the [`Action`] trait, and the [`Actions`] list a
//! [`ResourceDef`](super::ResourceDef) declares them in.

use std::{future::Future, pin::Pin};

use topcoat::{Result, context::Cx};

use super::Resource;

/// A mutation beyond create, update and delete: "publish", "archive",
/// "resend the invite".
///
/// An action runs on one record, from a button in its row, or on the
/// selection, from the bulk bar, or both ([`ROW`](Self::ROW),
/// [`BULK`](Self::BULK)). [`ResourceDef::action`](super::ResourceDef::action) declares it:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     title: String,
/// #     status: String,
/// # }
/// # use tablo_core::{Action, NoForm, Resource};
/// # use toasty::Executor;
/// # use topcoat::{Result, context::Cx};
/// # struct PostResource;
/// #
/// # impl Resource for PostResource {
/// #     type Model = Post;
/// #     type Form = NoForm<Post>;
/// # }
/// struct Publish;
///
/// impl Action<PostResource> for Publish {
///     const NAME: &'static str = "publish";
///
///     fn label(_cx: &Cx) -> String {
///         "Publish".to_string()
///     }
///
///     fn can_run(_cx: &Cx, post: &Post) -> bool {
///         post.status != "published"
///     }
///
///     async fn run(_cx: &Cx, posts: &[Post], ex: &mut dyn Executor) -> Result<()> {
///         for post in posts {
///             Post::filter(Post::fields().id().eq(post.id))
///                 .update()
///                 .status("published".to_string())
///                 .exec(&mut *ex)
///                 .await?;
///         }
///         Ok(())
///     }
/// }
/// ```
///
/// The framework owns everything around [`run`](Self::run), as it does for
/// a delete:
///
/// - the route, `{list}/{key}/-/actions/{NAME}` for a row and `{list}/-/actions/{NAME}` for the
///   selection, and its CSRF check;
/// - the transaction: the records are loaded through [`scoped_query`](super::scoped_query) inside
///   it, `run` writes through the same executor, and an error rolls everything back;
/// - the policy: every record must pass [`Ability::View`](crate::policy::Ability::View) and
///   [`can_run`](Self::can_run), checked on the loaded rows before `run`;
/// - [`Resource::after_commit`] with [`Mutation::Action`](super::Mutation::Action) once the
///   transaction commits, and the success notification.
///
/// A row whose record fails `can_run` renders no button for the action, and
/// a row that no bulk action and no delete allows renders no checkbox.
pub trait Action<R: Resource>: 'static {
    /// The action's URL segment, distinct among the resource's actions.
    ///
    /// [`ResourceDef::action`](super::ResourceDef::action) refuses to compile a name that is not
    /// a single path segment: empty, `.` or `..`, or holding whitespace, a control character, a
    /// quote, a backslash or one of `/ ? # % & = { } ( )`.
    /// [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel) refuses a name that
    /// another action of the resource shares.
    ///
    /// ```rust,compile_fail
    /// # use tablo_core::{Action, NoForm, Resource, ResourceDef};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// # }
    /// struct Archive;
    ///
    /// impl Action<PostResource> for Archive {
    ///     const NAME: &'static str = "archive/all";
    /// #   fn label(_cx: &Cx) -> String { String::new() }
    /// #   fn can_run(_: &Cx, _: &Post) -> bool { true }
    /// #   async fn run(_: &Cx, _: &[Post], _: &mut dyn toasty::Executor) -> Result<()> { Ok(()) }
    /// }
    ///
    /// let def = ResourceDef::<PostResource>::new().action::<Archive>();
    /// ```
    const NAME: &'static str;

    /// Whether a row renders the action's button. Defaults to `true`.
    const ROW: bool = true;

    /// Whether the bulk bar renders the action for the selection. Defaults
    /// to `true`.
    const BULK: bool = true;

    /// Whether the action asks first through a confirmation dialog sharing the
    /// delete dialog's mechanism and destructive wording. Defaults to `false`.
    ///
    /// An unconfirmed POST answers 400.
    const CONFIRM: bool = false;

    /// The button text.
    fn label(cx: &Cx) -> String;

    /// Whether the action may run on `record`.
    fn can_run(_cx: &Cx, _record: &R::Model) -> bool {
        true
    }

    /// Perform the action on `records`, all of which passed
    /// [`can_run`](Self::can_run), through the framework's transaction `ex`.
    fn run(
        cx: &Cx,
        records: &[R::Model],
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = Result<()>> + Send;

    /// The success notification after a commit. Defaults to the label and
    /// the record count: `"Publish: 3 records"`.
    fn success(cx: &Cx, count: usize) -> String {
        let noun = if count == 1 { "record" } else { "records" };
        format!("{}: {count} {noun}", Self::label(cx))
    }
}

/// What an erased action's `run` returns.
pub(crate) type ActionFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// The actions a [`Resource`] declares, in button order.
pub(crate) struct Actions<R: Resource> {
    entries: Vec<ActionEntry<R>>,
}

impl<R: Resource> Default for Actions<R> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<R: Resource> std::fmt::Debug for Actions<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.entries.iter().map(|e| e.name))
            .finish()
    }
}

impl<R: Resource> Actions<R> {
    /// Appends the action `A`, failing to compile when `A::NAME` is not a single path segment.
    pub(crate) fn add<A: Action<R>>(mut self) -> Self {
        const {
            assert!(
                crate::declaration::segment_fault(A::NAME).is_none(),
                "`Action::NAME` must be a single path segment"
            );
        }
        self.entries.push(ActionEntry {
            name: A::NAME,
            label: A::label,
            row: A::ROW,
            bulk: A::BULK,
            can_run: A::can_run,
            run: run_erased::<R, A>,
            success: A::success,
            acted: super::Committed::acted::<R, A>,
            confirm: A::CONFIRM,
        });
        self
    }

    /// The declared actions, in order.
    pub(crate) fn entries(&self) -> &[ActionEntry<R>] {
        &self.entries
    }

    /// The action named `name`.
    pub(crate) fn find(&self, name: &str) -> Option<&ActionEntry<R>> {
        self.entries.iter().find(|e| e.name == name)
    }
}

/// One declared action with its type erased, so a resource's actions sit
/// in one list.
pub(crate) struct ActionEntry<R: Resource> {
    pub(crate) name: &'static str,
    pub(crate) label: fn(&Cx) -> String,
    pub(crate) row: bool,
    pub(crate) bulk: bool,
    pub(crate) can_run: fn(&Cx, &R::Model) -> bool,
    pub(crate) run:
        for<'a> fn(&'a Cx, &'a [R::Model], &'a mut dyn toasty::Executor) -> ActionFuture<'a>,
    pub(crate) success: fn(&Cx, usize) -> String,
    pub(crate) acted: fn(Vec<R::Model>) -> super::Committed<R::Model>,
    pub(crate) confirm: bool,
}

/// [`Action::run`] behind a function pointer.
fn run_erased<'a, R: Resource, A: Action<R>>(
    cx: &'a Cx,
    records: &'a [R::Model],
    ex: &'a mut dyn toasty::Executor,
) -> ActionFuture<'a> {
    Box::pin(A::run(cx, records, ex))
}
