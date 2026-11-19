//! What a job is, and the type-erased run a worker keeps for each kind.

use std::{future::Future, pin::Pin, sync::Arc};

use serde::{Serialize, de::DeserializeOwned};
use topcoat::context::Cx;

/// A kind of background work, whose fields are its payload.
///
/// ```rust
/// use serde::{Deserialize, Serialize};
/// use tablo_jobs::Job;
/// use topcoat::context::Cx;
///
/// #[derive(Serialize, Deserialize)]
/// struct SyncSeats {
///     team: u64,
/// }
///
/// impl Job for SyncSeats {
///     const KIND: &'static str = "sync-seats";
///
///     fn lane(&self) -> Option<String> {
///         Some(format!("team:{}", self.team))
///     }
///
///     async fn run(self, cx: &Cx) -> topcoat::Result<()> {
///         // Read the team as it is now, and push its seat count.
///         # let _ = cx;
///         Ok(())
///     }
/// }
/// ```
///
/// - `run` may run twice for one enqueue, after a crash or a lapsed lock, so it is idempotent.
/// - It reads the world as it is when it runs, not as it was at enqueue: the payload names rows, it
///   does not copy them.
/// - The payload lives in the database and its backups in clear: it carries no secret.
pub trait Job: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// What the queue calls this kind. A queued job of a kind no worker registers fails, so a
    /// rename keeps the old name until the queue holds none.
    const KIND: &'static str;
    /// How many runs it gets before it turns [`Dead`](crate::JobState::Dead).
    const MAX_ATTEMPTS: u32 = 8;

    /// The lane it runs in: of the jobs sharing a lane, one runs at a time across every worker.
    /// `None`, the default, runs beside anything.
    fn lane(&self) -> Option<String> {
        None
    }

    /// Does the work, reading what it needs from `cx`: the context
    /// [`PanelHandle::context`](tablo_core::PanelHandle::context) builds, with what
    /// [`Worker::context`](crate::Worker::context) adds. An error retries it with backoff until
    /// it has spent its attempts.
    fn run(self, cx: &Cx) -> impl Future<Output = topcoat::Result<()>> + Send;
}

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// One kind's run: its payload parsed, then run against a context.
pub(crate) type Run =
    Arc<dyn for<'a> Fn(&'a str, &'a Cx) -> BoxFuture<'a, topcoat::Result<()>> + Send + Sync>;

/// `J`'s run.
pub(crate) fn run_of<J: Job>() -> Run {
    Arc::new(|payload, cx| {
        Box::pin(async move { serde_json::from_str::<J>(payload)?.run(cx).await })
    })
}
