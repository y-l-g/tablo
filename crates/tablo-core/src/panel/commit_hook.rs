//! [`Panel::before_commit`]: the hooks every write of a panel runs inside its transaction, whichever
//! resource wrote it.

use std::future::Future;

use toasty::Executor;
use topcoat::{Result, context::Cx};

use super::{Panel, state::current};
use crate::{
    BoxFuture, Text,
    resource::{Change, Resource, require_mounted},
};

/// Work every write of a panel does inside its transaction, whichever of the panel's resources
/// wrote it: an audit trail no resource can leave out. [`Panel::before_commit`] registers it.
///
/// It runs on the writes [`Resource::before_commit`] runs on, right after that hook, with the
/// same transaction and the same rules: run every statement through `ex`, and an `Err` rolls the
/// whole write back and fails the request with the write's failure toast.
///
/// ```rust
/// # use tablo_core::{Change, CommitHook, PanelChange};
/// # use topcoat::{Result, context::Cx};
/// # #[derive(Debug, toasty::Model)]
/// # struct AuditEntry { #[key] #[auto] id: uuid::Uuid, resource: String, record: String }
/// struct Audit;
///
/// impl CommitHook for Audit {
///     async fn before_commit(
///         cx: &Cx,
///         change: &PanelChange,
///         ex: &mut dyn toasty::Executor,
///     ) -> Result<()> {
///         for record in change.change().records() {
///             toasty::create!(AuditEntry {
///                 resource: change.label().localize(cx),
///                 record: record.key().to_string(),
///             })
///             .exec(&mut *ex)
///             .await?;
///         }
///         Ok(())
///     }
/// }
/// ```
pub trait CommitHook: 'static {
    /// Work inside the write's transaction, after the resource's own
    /// [`Resource::before_commit`].
    fn before_commit(
        cx: &Cx,
        change: &PanelChange,
        ex: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// A [`Change`] as a [`CommitHook`] receives it: the resource that wrote it, and each record it
/// names by its key and its URL.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelChange {
    slug: String,
    label: Text,
    change: Change<ChangedRecord>,
}

impl PanelChange {
    /// The slug of the resource that wrote, as its URLs spell it: `posts`.
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// The resource's label: `Post`, or the message its catalog spells it with.
    pub fn label(&self) -> &Text {
        &self.label
    }

    /// What the write changed, each record named by its key and URL.
    pub fn change(&self) -> &Change<ChangedRecord> {
        &self.change
    }
}

/// A record a [`PanelChange`] names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedRecord {
    key: String,
    url: Option<String>,
}

impl ChangedRecord {
    /// The record's key, as its URLs spell it: `42`.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The record's detail page, as [`url::record`](crate::url::record) spells it; `None` when
    /// the resource serves none.
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
}

/// A registered [`CommitHook`], its type erased.
pub(crate) type CommitHookFn =
    for<'a> fn(&'a Cx, &'a PanelChange, &'a mut dyn Executor) -> BoxFuture<'a, Result<()>>;

fn erased<'a, H: CommitHook>(
    cx: &'a Cx,
    change: &'a PanelChange,
    ex: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<()>> {
    Box::pin(H::before_commit(cx, change, ex))
}

impl Panel {
    /// Runs the [`CommitHook`] `H` inside the transaction of every write of the panel, after the
    /// writing resource's own [`Resource::before_commit`]. Several hooks run in the order they
    /// are registered.
    pub fn before_commit<H: CommitHook>(mut self) -> Self {
        self.commit_hooks.push(erased::<H>);
        self
    }
}

/// Run `R`'s [`Resource::before_commit`] on `change`, then each of the panel's [`CommitHook`]s,
/// inside the write's transaction `ex`.
pub(crate) async fn before_commit<R: Resource>(
    cx: &Cx,
    change: &Change<R::Model>,
    ex: &mut dyn Executor,
) -> Result<()> {
    R::before_commit(cx, change, &mut *ex).await?;
    let hooks = current(cx).map_or(&[][..], |panel| panel.commit_hooks.as_slice());
    if hooks.is_empty() {
        return Ok(());
    }
    let mounted = require_mounted::<R>(cx)?;
    let change = PanelChange {
        slug: mounted.slug.clone(),
        label: mounted.label.clone(),
        change: change.map(|record| ChangedRecord {
            key: mounted.table.key_of(record),
            url: crate::url::record::<R>(cx, record),
        }),
    };
    run(cx, hooks, &change, ex).await
}

/// Run each of `hooks` on `change`, stopping at the first error.
fn run<'a>(
    cx: &'a Cx,
    hooks: &'a [CommitHookFn],
    change: &'a PanelChange,
    ex: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<()>> {
    Box::pin(async move {
        for hook in hooks {
            hook(cx, change, &mut *ex).await?;
        }
        Ok(())
    })
}
