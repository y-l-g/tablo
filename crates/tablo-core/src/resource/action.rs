//! Custom actions: the [`Action`] trait, and the [`Actions`] list a
//! [`Resource`] declares them in.

use std::{future::Future, pin::Pin};

use topcoat::{Result, context::Cx};

use super::Resource;

/// A mutation beyond create, update and delete: "publish", "archive",
/// "resend the invite".
///
/// An action runs on one record, from a button in its row, or on the
/// selection, from the bulk bar, or both ([`ROW`](Self::ROW),
/// [`BULK`](Self::BULK)). [`Resource::actions`] declares it:
///
/// ```ignore
/// struct Publish;
///
/// impl Action<PostResource> for Publish {
///     const NAME: &'static str = "publish";
///
///     fn label() -> String {
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
/// - the route, `{list}/{key}/actions/{NAME}` for a row and `{list}/actions/{NAME}` for the
///   selection, and its CSRF check;
/// - the transaction: the records are loaded through [`scoped_query`](super::scoped_query) inside
///   it, `run` writes through the same executor, and an error rolls everything back;
/// - the policy: every record must pass [`Resource::can_view`] and [`can_run`](Self::can_run),
///   checked on the loaded rows before `run`;
/// - [`Resource::after_commit`] with [`Mutation::Action`](super::Mutation::Action) once the
///   transaction commits, and the success notification.
///
/// A row whose record fails `can_run` renders no button for the action, and
/// a row that no bulk action and no delete allows renders no checkbox.
pub trait Action<R: Resource>: 'static {
    /// The action's URL segment, distinct among the resource's actions.
    /// [`Panel::build`](crate::Panel::build) refuses one that is not a
    /// single path segment, or that another action of the resource shares.
    const NAME: &'static str;

    /// Whether a row renders the action's button. Defaults to `true`.
    const ROW: bool = true;

    /// Whether the bulk bar renders the action for the selection. Defaults
    /// to `true`.
    const BULK: bool = true;

    /// The button text.
    fn label() -> String;

    /// Whether the action may run on `record`.
    fn can_run(cx: &Cx, record: &R::Model) -> bool;

    /// Perform the action on `records`, all of which passed
    /// [`can_run`](Self::can_run), through the framework's transaction `ex`.
    fn run(
        cx: &Cx,
        records: &[R::Model],
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = Result<()>> + Send;

    /// The success notification after a commit. Defaults to the label and
    /// the record count: `"Publish: 3 records"`.
    fn success(count: usize) -> String {
        let noun = if count == 1 { "record" } else { "records" };
        format!("{}: {count} {noun}", Self::label())
    }
}

/// What an erased action's `run` returns.
pub(crate) type ActionFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// The actions a [`Resource`] declares, in button order:
/// `Actions::new().add::<Publish>().add::<Archive>()`.
pub struct Actions<R: Resource> {
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
    /// No actions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append the action `A`.
    pub fn add<A: Action<R>>(mut self) -> Self {
        self.entries.push(ActionEntry {
            name: A::NAME,
            label: A::label,
            row: A::ROW,
            bulk: A::BULK,
            can_run: A::can_run,
            run: run_erased::<R, A>,
            success: A::success,
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
    pub(crate) label: fn() -> String,
    pub(crate) row: bool,
    pub(crate) bulk: bool,
    pub(crate) can_run: fn(&Cx, &R::Model) -> bool,
    pub(crate) run:
        for<'a> fn(&'a Cx, &'a [R::Model], &'a mut dyn toasty::Executor) -> ActionFuture<'a>,
    pub(crate) success: fn(usize) -> String,
}

/// [`Action::run`] behind a function pointer.
fn run_erased<'a, R: Resource, A: Action<R>>(
    cx: &'a Cx,
    records: &'a [R::Model],
    ex: &'a mut dyn toasty::Executor,
) -> ActionFuture<'a> {
    Box::pin(A::run(cx, records, ex))
}
