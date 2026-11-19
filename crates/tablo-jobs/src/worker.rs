//! Claiming and running jobs.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use tablo_core::PanelHandle;
use toasty::Db;
use tokio::{
    sync::watch,
    task::{JoinError, JoinSet},
};
use topcoat::context::Cx;
use tracing::Instrument;

use crate::{
    AuthMailQueue, Job, JobState, QueuedJob,
    job::{Kind, kind_of},
    mail::SendAuthMail,
    schedule::Schedule,
};

/// How long a lock outlives its kind's timeout, so no live run is taken over.
const LOCK_MARGIN: Duration = Duration::from_mins(1);
/// How many jobs one worker runs at once.
const BATCH: usize = 16;
/// How long a worker waits before it asks again, when no run of its own ends first.
const IDLE: Duration = Duration::from_secs(1);
/// How often a worker sweeps for lapsed locks and due schedules, however busy it is.
const SWEEP: Duration = Duration::from_secs(15);
const BACKOFF_BASE_SECONDS: i64 = 30;
const BACKOFF_CAP_SECONDS: i64 = 60 * 60;
/// How long a succeeded job stays by default.
const KEEP_SUCCEEDED: Duration = Duration::from_hours(14 * 24);
/// How often the queue deletes the succeeded jobs past their keep.
const PRUNE_EVERY: Duration = Duration::from_hours(24);

/// Runs a panel's queued jobs: claims the due ones, runs each with a timeout against the panel's
/// [`context`](PanelHandle::context), and retries a failure with backoff.
///
/// ```rust,no_run
/// # use serde::{Deserialize, Serialize};
/// # use std::time::Duration;
/// # use tablo_core::PanelHandle;
/// # use tablo_jobs::{AuthMailQueue, Job, Worker};
/// # #[derive(Clone, Serialize, Deserialize)]
/// # struct ReconcileBilling;
/// # impl Job for ReconcileBilling {
/// #     const KIND: &'static str = "reconcile-billing";
/// #     async fn run(self, _: &topcoat::context::Cx) -> topcoat::Result<()> { Ok(()) }
/// # }
/// # #[derive(Clone)] struct Stripe;
/// # async fn start(panel: &PanelHandle, mail: AuthMailQueue, stripe: Stripe) -> topcoat::Result<()> {
/// let (stop, stopped) = tokio::sync::watch::channel(false);
/// let worker = Worker::new(panel)?
///     .every(Duration::from_hours(6), ReconcileBilling)
///     .auth_mail(&mail)
///     .context(move |cx| cx.with(stripe.clone()));
/// let running = tokio::spawn(worker.run(stopped));
/// // On a shutdown signal:
/// stop.send(true).ok();
/// running.await.ok();
/// # Ok(())
/// # }
/// ```
///
/// - Delivery is at least once. A worker that stops mid-run leaves the job's lock to lapse, a
///   minute past its kind's [`TIMEOUT`](Job::TIMEOUT), and the job runs again after its backoff.
/// - A run that fails, or outlasts its [`TIMEOUT`](Job::TIMEOUT), waits from 30 seconds, doubling
///   up to an hour, with up to a quarter more drawn from the job's id. A job out of attempts turns
///   [`Dead`](JobState::Dead) and stays until someone queues it again.
/// - A worker claims only the kinds it [`register`](Self::register)s, so workers may split the
///   kinds between them. A job of a kind no worker registers, such as one a newer release queued,
///   waits for a worker that does.
/// - Any number of workers share one queue. A claim is a version-checked update, so each job runs
///   once per claim, and the running job of a [`lane`](Job::lane) holds the lane in a unique
///   column, so the database refuses a second.
/// - Every day it deletes the jobs that succeeded more than
///   [`keep_succeeded`](Self::keep_succeeded) ago, fourteen days by default. A dead job stays.
#[derive(Clone)]
pub struct Worker {
    inner: Arc<Inner>,
}

#[derive(Clone)]
struct Inner {
    db: Db,
    panel: PanelHandle,
    kinds: HashMap<&'static str, Kind>,
    schedules: Vec<Schedule>,
    context: Arc<dyn Fn(Cx) -> Cx + Send + Sync>,
    mail: Option<AuthMailQueue>,
}

impl Worker {
    /// A worker running jobs in `panel`'s database, against `panel`'s context.
    ///
    /// # Errors
    ///
    /// When `panel`'s context holds no database, which a handle from
    /// [`Panel::handle`](tablo_core::Panel::handle) or a router always does.
    pub fn new(panel: &PanelHandle) -> topcoat::Result<Self> {
        let db = tablo_core::db::db(&panel.context())?;
        let worker = Self {
            inner: Arc::new(Inner {
                db,
                panel: panel.clone(),
                kinds: HashMap::new(),
                schedules: Vec::new(),
                context: Arc::new(|cx| cx),
                mail: None,
            }),
        };
        Ok(worker.keep_succeeded(KEEP_SUCCEEDED))
    }

    fn inner_mut(&mut self) -> &mut Inner {
        Arc::make_mut(&mut self.inner)
    }

    /// Runs the jobs of kind `J`. Registering a kind again changes nothing.
    #[must_use]
    pub fn register<J: Job>(mut self) -> Self {
        self.inner_mut().kinds.insert(J::KIND, kind_of::<J>());
        self
    }

    /// Registers `J` and enqueues `job` every `every`, the first run one interval after any
    /// worker first sees the schedule. However many workers run, each period enqueues once, and
    /// a run that has not started absorbs the next. Scheduling a kind again replaces its schedule.
    #[must_use]
    pub fn every<J: Job + Clone>(mut self, every: Duration, job: J) -> Self {
        let inner = self.inner_mut();
        inner
            .schedules
            .retain(|schedule| schedule.kind() != J::KIND);
        inner.schedules.push(Schedule::new(every, job));
        self.register::<J>()
    }

    /// Adds to each run's context what its jobs read beyond the panel's: a mail transport, an API
    /// client. `extend` receives the panel's context and returns it with its additions. Each run
    /// gets a context of its own, since a context memoizes what it loads. Calling it again
    /// replaces the previous `extend`.
    #[must_use]
    pub fn context(mut self, extend: impl Fn(Cx) -> Cx + Send + Sync + 'static) -> Self {
        self.inner_mut().context = Arc::new(extend);
        self
    }

    /// Sends the letters `queue` queues, through the [`Mailer`](topcoat::mail::Mailer) the
    /// [`context`](Self::context) adds. See [`AuthMailQueue`].
    #[must_use]
    pub fn auth_mail(mut self, queue: &AuthMailQueue) -> Self {
        self.inner_mut().mail = Some(queue.clone());
        self.register::<SendAuthMail>()
    }

    /// Keeps a succeeded job for `age` before the daily prune deletes it.
    #[must_use]
    pub fn keep_succeeded(self, age: Duration) -> Self {
        self.every(
            PRUNE_EVERY,
            PruneJobs {
                keep_seconds: age.as_secs(),
            },
        )
    }

    /// Polls for due jobs until `shutdown` holds `true` or its sender drops, then lets the runs
    /// in hand finish. A slot a run frees takes the next due job at once, and the sweep keeps its
    /// pace under any load.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut runs = JoinSet::new();
        let mut swept: Option<Instant> = None;
        while !*shutdown.borrow() {
            if swept.is_none_or(|at| at.elapsed() >= SWEEP) {
                swept = Some(Instant::now());
                self.sweep().await;
            }
            let free = BATCH - runs.len();
            if free > 0 {
                match self.claim(free).await {
                    Ok(claimed) => {
                        for job in claimed {
                            self.spawn(&mut runs, job);
                        }
                    }
                    Err(error) => {
                        tracing::error!(error = %format!("{error:#}"), "claiming jobs failed");
                    }
                }
            }
            tokio::select! {
                () = tokio::time::sleep(IDLE) => {}
                Some(run) = runs.join_next(), if !runs.is_empty() => ended(run),
                changed = shutdown.changed() => if changed.is_err() { break },
            }
        }
        while let Some(run) = runs.join_next().await {
            ended(run);
        }
    }

    /// Requeues the jobs whose lock lapsed and enqueues the due schedules, then runs every due
    /// job until none is left: what a test calls to say "the worker ran".
    ///
    /// # Errors
    ///
    /// When the driver fails to claim jobs.
    pub async fn drain(&self) -> topcoat::Result<()> {
        self.sweep().await;
        while self.drain_once().await? > 0 {}
        Ok(())
    }

    /// Claims up to a batch of due jobs, runs them to their end, and answers how many it claimed.
    ///
    /// # Errors
    ///
    /// When the driver fails to claim jobs. A job's own failure is recorded on its row instead.
    pub async fn drain_once(&self) -> topcoat::Result<usize> {
        let claimed = self.claim(BATCH).await?;
        let count = claimed.len();
        let mut runs = JoinSet::new();
        for job in claimed {
            self.spawn(&mut runs, job);
        }
        while let Some(run) = runs.join_next().await {
            ended(run);
        }
        Ok(count)
    }

    fn spawn(&self, runs: &mut JoinSet<()>, job: QueuedJob) {
        let worker = self.clone();
        let span = tracing::info_span!(
            "job",
            kind = %job.kind,
            id = %job.id,
            attempt = job.attempts
        );
        runs.spawn(async move { worker.execute(job).await }.instrument(span));
    }

    /// Requeues lapsed locks and enqueues due schedules. A failure waits for the next sweep.
    async fn sweep(&self) {
        if let Err(error) = self.recover().await {
            tracing::error!(error = %format!("{error:#}"), "recovering lapsed jobs failed");
        }
        for schedule in &self.inner.schedules {
            if let Err(error) = schedule.tick(&self.inner.db).await {
                tracing::error!(
                    kind = schedule.kind(),
                    error = %format!("{error:#}"),
                    "a schedule failed"
                );
            }
        }
    }

    /// Up to `limit` due jobs of the registered kinds, each now held by this worker.
    async fn claim(&self, limit: usize) -> topcoat::Result<Vec<QueuedJob>> {
        let mut db = self.inner.db.clone();
        let now = Timestamp::now();
        let fields = QueuedJob::fields();
        // Leaves out the jobs of busy lanes, so they do not fill the batch and hold back the rest.
        // Exclusion itself is the unique `running_lane`: this read is stale once another worker
        // claims.
        let mut busy: HashSet<String> = QueuedJob::filter(fields.state().eq(JobState::Running))
            .exec(&mut db)
            .await?
            .into_iter()
            .filter_map(|job| job.running_lane)
            .collect();
        let kinds: Vec<String> = self.inner.kinds.keys().map(ToString::to_string).collect();
        let mut due = fields
            .state()
            .eq(JobState::Queued)
            .and(fields.run_at().le(now))
            .and(fields.kind().in_list(kinds));
        if !busy.is_empty() {
            let lanes: Vec<Option<String>> = busy.iter().cloned().map(Some).collect();
            due = due.and(
                fields
                    .lane()
                    .is_none()
                    .or(fields.lane().in_list(lanes).not()),
            );
        }
        let due = QueuedJob::filter(due)
            .order_by(fields.run_at().asc())
            .limit(limit)
            .exec(&mut db)
            .await?;
        let mut claimed = Vec::with_capacity(due.len());
        for mut job in due {
            if job.lane.as_ref().is_some_and(|lane| busy.contains(lane)) {
                continue;
            }
            let Some(kind) = self.inner.kinds.get(job.kind.as_str()) else {
                continue;
            };
            let locked_until = now.checked_add(SignedDuration::try_from(
                kind.timeout.saturating_add(LOCK_MARGIN),
            )?)?;
            let free_key = job.id.to_string();
            let attempts = job.attempts + 1;
            let lane = job.lane.clone();
            // Lost to the version check when another worker claimed this job, and to the unique
            // `running_lane` when another worker claimed a job of its lane.
            let update = job
                .update()
                .state(JobState::Running)
                .running_lane(lane.clone())
                .attempts(attempts)
                .locked_until(Some(locked_until))
                .dedupe_key(free_key)
                .exec(&mut db)
                .await;
            match update {
                Ok(()) => {
                    busy.extend(lane);
                    claimed.push(job);
                }
                Err(error) if lost_race(&error) => {}
                // The jobs claimed so far still run; the rest wait for the next claim.
                Err(error) if !claimed.is_empty() => {
                    tracing::error!(%error, "claiming a job failed");
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(claimed)
    }

    /// Runs `job` and records how it ended.
    async fn execute(&self, job: QueuedJob) {
        // A claim takes only registered kinds.
        let Some(kind) = self.inner.kinds.get(job.kind.as_str()) else {
            return;
        };
        let cx = self.run_context();
        let outcome = tokio::time::timeout(kind.timeout, (kind.run)(&job.payload, &cx))
            .await
            .unwrap_or_else(|_| {
                Err(topcoat::Error::msg(format!(
                    "timed out after {:?}",
                    kind.timeout
                )))
            });
        if let Err(error) = self.record(job, outcome).await {
            // The lock lapses and the job runs again, which an idempotent job survives.
            tracing::error!(error = %format!("{error:#}"), "recording a job's outcome failed");
        }
    }

    /// The context one run reads.
    fn run_context(&self) -> Cx {
        let cx = (self.inner.context)(self.inner.panel.context());
        match &self.inner.mail {
            Some(mail) => cx.with(mail.clone()),
            None => cx,
        }
    }

    async fn record(
        &self,
        mut job: QueuedJob,
        outcome: topcoat::Result<()>,
    ) -> topcoat::Result<()> {
        let mut db = self.inner.db.clone();
        let now = Timestamp::now();
        let Err(error) = outcome else {
            job.update()
                .state(JobState::Succeeded)
                .running_lane(None)
                .locked_until(None)
                .finished_at(Some(now))
                .exec(&mut db)
                .await?;
            return Ok(());
        };
        let message = format!("{error:#}");
        if job.attempts >= job.max_attempts {
            tracing::error!(error = %message, "a job is dead");
            job.update()
                .state(JobState::Dead)
                .running_lane(None)
                .locked_until(None)
                .finished_at(Some(now))
                .last_error(Some(message))
                .exec(&mut db)
                .await?;
        } else {
            tracing::warn!(error = %message, "a job will retry");
            let run_at = now.checked_add(backoff(job.attempts, job.id))?;
            job.update()
                .state(JobState::Queued)
                .running_lane(None)
                .locked_until(None)
                .run_at(run_at)
                .last_error(Some(message))
                .exec(&mut db)
                .await?;
        }
        Ok(())
    }

    /// Requeues the running jobs whose lock lapsed: their worker stopped.
    async fn recover(&self) -> topcoat::Result<()> {
        let mut db = self.inner.db.clone();
        let fields = QueuedJob::fields();
        let lapsed = QueuedJob::filter(
            fields
                .state()
                .eq(JobState::Running)
                .and(fields.locked_until().le(Timestamp::now())),
        )
        .exec(&mut db)
        .await?;
        let now = Timestamp::now();
        for mut job in lapsed {
            tracing::warn!(kind = %job.kind, id = %job.id, "a job's lock lapsed");
            let message = Some("the worker running it stopped".to_string());
            // Spent like a failed run: dead when out of attempts, else retried after its backoff.
            let update = if job.attempts >= job.max_attempts {
                job.update()
                    .state(JobState::Dead)
                    .running_lane(None)
                    .locked_until(None)
                    .finished_at(Some(now))
                    .last_error(message)
                    .exec(&mut db)
                    .await
            } else {
                let run_at = now.checked_add(backoff(job.attempts, job.id))?;
                job.update()
                    .state(JobState::Queued)
                    .running_lane(None)
                    .locked_until(None)
                    .run_at(run_at)
                    .last_error(message)
                    .exec(&mut db)
                    .await
            };
            match update {
                Ok(()) => {}
                // Another worker recovered it first.
                Err(error) if lost_race(&error) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

/// Reports a run's task that ended abnormally.
fn ended(run: Result<(), JoinError>) {
    if let Err(error) = run {
        // A panicking job keeps its lock until it lapses, then runs again.
        tracing::error!(%error, "a job's task ended abnormally");
    }
}

/// Whether `error` is a write another worker won: the version check or a unique index refused
/// it. A lost connection is the driver failing instead.
pub(crate) fn lost_race(error: &toasty::Error) -> bool {
    !(error.is_connection_lost() || error.is_connection_pool() || error.is_transaction_timeout())
}

/// How long the run after `attempt` waits: doubling from 30 seconds to an hour, with up to a
/// quarter more drawn from the job's id, so a burst of failures does not retry as one.
fn backoff(attempt: u64, id: uuid::Uuid) -> SignedDuration {
    let exponent = u32::try_from(attempt.saturating_sub(1))
        .unwrap_or(u32::MAX)
        .min(16);
    let seconds = BACKOFF_BASE_SECONDS
        .saturating_mul(1 << exponent)
        .min(BACKOFF_CAP_SECONDS);
    let spread = i64::from(id.as_bytes()[15]) * seconds / (4 * 255);
    SignedDuration::from_secs(seconds + spread)
}

/// Deletes the jobs that succeeded more than `keep_seconds` ago.
#[derive(Clone, Serialize, Deserialize)]
struct PruneJobs {
    keep_seconds: u64,
}

impl Job for PruneJobs {
    const KIND: &'static str = "tablo-prune-jobs";

    async fn run(self, cx: &Cx) -> topcoat::Result<()> {
        let keep = SignedDuration::try_from(Duration::from_secs(self.keep_seconds))?;
        let cutoff = Timestamp::now().checked_sub(keep)?;
        let fields = QueuedJob::fields();
        QueuedJob::filter(
            fields
                .state()
                .eq(JobState::Succeeded)
                .and(fields.finished_at().le(cutoff)),
        )
        .delete()
        .exec(&mut tablo_core::db::db(cx)?)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
