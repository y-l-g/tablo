//! A header action's POST: the same reading of the POST as [`mutate`](super::mutate), and one
//! transaction with no record to load.

use topcoat::{context::Cx, router::Body, view::BoxView};

use super::{InputPage, Pending, Posted, read_post, refusal};
use crate::{
    BoxFuture,
    db::{WriteTx, db},
    form::FormValues,
    panel::write::commit_then,
    resource::{Change, HeaderEntry, Resource, WriteError, run_after_commit},
};

/// A resource's hooks around a header action's commit, erased so [`run_header`] compiles once.
#[derive(Clone, Copy)]
pub(crate) struct HeaderHooks {
    before: for<'c> fn(
        &'c Cx,
        &'static str,
        &'c mut dyn toasty::Executor,
    ) -> BoxFuture<'c, topcoat::Result<()>>,
    after: for<'c> fn(&'c Cx, &'static str) -> BoxFuture<'c, ()>,
}

impl HeaderHooks {
    /// `R`'s [`Resource::before_commit`] and [`Resource::after_commit`], for its header actions.
    pub(super) fn of<R: Resource>() -> Self {
        Self {
            before: |cx, name, ex| {
                Box::pin(
                    async move { R::before_commit(cx, &Change::HeaderAction { name }, ex).await },
                )
            },
            after: |cx, name| Box::pin(run_after_commit::<R>(cx, Change::HeaderAction { name })),
        }
    }
}

/// Runs the header action `action`, whose resource-wide or page check already passed, landing on
/// `home` or the POST's `?return=`, and runs a resource's `hooks` around its commit.
///
/// Boxed for the reason the list's `Frame::render_page` is: only generic handlers poll it.
pub(crate) fn run_header<'f, 'a: 'f>(
    cx: &'a Cx,
    body: Body,
    action: HeaderEntry,
    home: &'f str,
    hooks: Option<HeaderHooks>,
) -> BoxFuture<'f, Result<BoxView<'a>, topcoat::Error>> {
    Box::pin(header_run(cx, body, action, home, hooks))
}

async fn header_run<'a>(
    cx: &'a Cx,
    body: Body,
    action: HeaderEntry,
    home: &str,
    hooks: Option<HeaderHooks>,
) -> Result<BoxView<'a>, topcoat::Error> {
    let Posted { pending, .. } =
        read_post(cx, body, action.name, action.confirm, &action.input).await?;
    let render = |page: InputPage| {
        let label = (action.label)(cx);
        let chrome = page.chrome(
            action.name,
            label.clone(),
            label,
            action.confirm,
            Vec::new(),
            home,
        );
        (page, chrome)
    };
    let (input, values) = match pending {
        Pending::Ready(input) => (input, FormValues::new()),
        Pending::Submitted { input, values } => (input, values),
        Pending::Page(page) => {
            let (page, chrome) = render(page);
            return page.render(cx, &action.input, chrome).await;
        }
    };
    let mut db = db(cx);
    let mut tx = WriteTx::begin(cx, &mut db)
        .await
        .map_err(crate::error::unavailable)?;
    let errors = (action.input.schema)()
        .recheck_relationships(cx, &values, &FormValues::new(), &mut *tx)
        .await;
    if !errors.is_empty() {
        // The page's choices may query: release the connection first.
        drop(tx);
        let (page, chrome) = render(InputPage { values, errors });
        return page.render(cx, &action.input, chrome).await;
    }
    let written = match ((action.run)(cx, input, &mut *tx).await, hooks) {
        (Err(WriteError::Refused(errors)), _) => {
            drop(tx);
            let errors = refusal(action.name, &action.input, errors)?;
            let (page, chrome) = render(InputPage { values, errors });
            return page.render(cx, &action.input, chrome).await;
        }
        (Err(WriteError::Failed(error)), _) => Err(error),
        (Ok(outcome), Some(hooks)) => (hooks.before)(cx, action.name, &mut *tx)
            .await
            .map(|()| outcome),
        (Ok(outcome), None) => Ok(outcome),
    };
    let note = (action.success)(cx);
    commit_then(
        cx,
        tx,
        written.map(|outcome| ((), outcome)),
        |()| async move {
            if let Some(hooks) = hooks {
                (hooks.after)(cx, action.name).await;
            }
        },
        note,
        "tablo-failed-run",
        home,
    )
    .await
}
