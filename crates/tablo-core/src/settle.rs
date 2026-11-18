//! The work a panel runs after its response, such as sending auth mail, and [`settled`], which
//! waits for it.

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use tokio::sync::Notify;
use topcoat::{
    context::{Cx, try_context},
    router::Router,
};

/// Waits until the work the router's panels run after their responses has finished: the auth
/// mail every request so far left behind is sent or handed to its delivery, or its failure
/// logged.
///
/// A panel sends its auth mail once the response has left, so that how long a request takes
/// tells no one whether an account exists. That work runs beside the server, which stops
/// waiting for it once its last response is sent. Await this after `topcoat::serve_until` or
/// `topcoat::start` returns, so a shutdown sends the mail already promised:
///
/// ```rust,no_run
/// # async fn run(router: topcoat::router::Router, serving: impl Future<Output = std::io::Result<()>>) -> std::io::Result<()> {
/// use std::time::Duration;
///
/// // `serving` is `topcoat::serve_until(listener, router.clone(), stop)`.
/// serving.await?;
/// if tokio::time::timeout(Duration::from_secs(30), tablo_core::settled(&router))
///     .await
///     .is_err()
/// {
///     tracing::warn!("stopping with auth mail still unsent");
/// }
/// # Ok(())
/// # }
/// ```
///
/// `tablo::testing::TestClient` awaits it after every request, so a test reads the mail a
/// request sent as soon as the request returns. A router that mounts no panel has no
/// such work, and this returns at once.
pub async fn settled(router: &Router) {
    if let Some(after) = router.context().get::<AfterResponse>() {
        after.settled().await;
    }
}

/// The router's work in flight after a response, shared by every panel it mounts. Cloning it
/// is cheap.
#[derive(Clone, Default)]
pub(crate) struct AfterResponse(Arc<Running>);

#[derive(Default)]
struct Running {
    count: AtomicUsize,
    idle: Notify,
}

impl AfterResponse {
    /// Runs `work` beside the request on the router `cx` serves, which [`settled`] waits for; on
    /// a context outside every router, beside nothing that waits.
    pub(crate) fn spawn(cx: &Cx, work: impl Future<Output = ()> + Send + 'static) {
        match try_context::<Self>(cx) {
            Some(after) => after.run(work),
            None => drop(tokio::spawn(work)),
        }
    }

    /// Runs `work` in a task of its own, counted until it ends.
    fn run(&self, work: impl Future<Output = ()> + Send + 'static) {
        // Counted before the task starts, so a `settled` right after the response sees it.
        let guard = Guard::new(Arc::clone(&self.0));
        tokio::spawn(async move {
            let _guard = guard;
            work.await;
        });
    }

    async fn settled(&self) {
        loop {
            let idle = self.0.idle.notified();
            let mut idle = std::pin::pin!(idle);
            // Registered before the count is read, so a task ending in between still wakes it.
            idle.as_mut().enable();
            if self.0.count.load(Ordering::Acquire) == 0 {
                return;
            }
            idle.await;
        }
    }
}

/// One unit of work in flight: counted while it lives, so a task that panics still ends.
struct Guard(Arc<Running>);

impl Guard {
    fn new(running: Arc<Running>) -> Self {
        running.count.fetch_add(1, Ordering::AcqRel);
        Self(running)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if self.0.count.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.0.idle.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests;
