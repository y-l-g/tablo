//! Claiming and running jobs.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use tablo_core::PanelHandle;
use toasty::Db;
use tokio::{sync::watch, task::JoinSet};
use topcoat::context::Cx;
use tracing::Instrument;

use crate::{
    AuthMailQueue, Job, JobState, QueuedJob,
    job::{Run, run_of},
    mail::SendAuthMail,
    schedule::Schedule,
};

/// How long a run may take before it counts as failed.
const RUN_TIMEOUT: Duration = Duration::from_mins(2);
/// A lock outlives the timeout, so no live run is taken over.
const LOCK: SignedDuration = SignedDuration::from_mins(3);
/// How many jobs one worker runs at once.
const BATCH: usize = 16;
/// How long an idle worker waits before it asks again.
const IDLE: Duration = Duration::from_secs(1);
/// How many idle polls pass between two sweeps for lapsed locks and due schedules.
const SWEEP_EVERY: u32 = 15;
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
/// - Delivery is at least once. A worker that stops mid-run leaves the job's lock to lapse, three
///   minutes after its claim, and the next worker runs it again.
/// - A run that fails, or outlasts two minutes, waits from 30 seconds, doubling up to an hour, with
///   up to a quarter more drawn from the job's id. A job out of attempts turns
///   [`Dead`](JobState::Dead) and stays until someone queues it again.
/// - A job of a kind the worker does not [`register`](Self::register) fails like any other, so a
///   worker built from an older release spends its attempts rather than drops it.
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
    runs: HashMap<&'static str, Run>,
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
                runs: HashMap::new(),
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
        self.inner_mut().runs.insert(J::KIND, run_of::<J>());
        self
    }

    /// Registers `J` and enqueues `job` every `every`, the first run one interval after any
    /// worker first sees the schedule. However many workers run, each period enqueues once, and
    /// a run still waiting absorbs the next. Scheduling a kind again replaces its schedule.
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
    /// in hand finish.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut idle = SWEEP_EVERY;
        while !*shutdown.borrow() {
            if idle >= SWEEP_EVERY {
                idle = 0;
                self.sweep().await;
            }
            match self.drain_once().await {
                Ok(0) => idle += 1,
                Ok(_) => continue,
                Err(error) => {
                    tracing::error!(error = %format!("{error:#}"), "claiming jobs failed");
                }
            }
            tokio::select! {
                () = tokio::time::sleep(IDLE) => {}
                changed = shutdown.changed() => if changed.is_err() { return },
            }
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
        let claimed = self.claim().await?;
        let count = claimed.len();
        let mut runs = JoinSet::new();
        for job in claimed {
            let worker = self.clone();
            let span = tracing::info_span!(
                "job",
                kind = %job.kind,
                id = %job.id,
                attempt = job.attempts
            );
            runs.spawn(async move { worker.execute(job).await }.instrument(span));
        }
        while let Some(run) = runs.join_next().await {
            if let Err(error) = run {
                // A panicking job keeps its lock until it lapses, then runs again.
                tracing::error!(%error, "a job's task ended abnormally");
            }
        }
        Ok(count)
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

    /// Up to a batch of due jobs, each now held by this worker.
    async fn claim(&self) -> topcoat::Result<Vec<QueuedJob>> {
        let mut db = self.inner.db.clone();
        let now = Timestamp::now();
        let fields = QueuedJob::fields();
        let due = QueuedJob::filter(
            fields
                .state()
                .eq(JobState::Queued)
                .and(fields.run_at().le(now)),
        )
        .order_by(fields.run_at().asc())
        .limit(BATCH)
        .exec(&mut db)
        .await?;
        if due.is_empty() {
            return Ok(Vec::new());
        }
        // Saves an update bound to fail for each held-back job. Exclusion itself is the unique
        // `running_lane`: this read is stale once another worker claims.
        let mut busy: HashSet<String> = QueuedJob::filter(fields.state().eq(JobState::Running))
            .exec(&mut db)
            .await?
            .into_iter()
            .filter_map(|job| job.running_lane)
            .collect();
        let locked_until = now.checked_add(LOCK)?;
        let mut claimed = Vec::with_capacity(due.len());
        for mut job in due {
            if job.lane.as_ref().is_some_and(|lane| busy.contains(lane)) {
                continue;
            }
            let free_key = job.id.to_string();
            let attempts = job.attempts + 1;
            let lane = job.lane.clone();
            // Lost to the version check when another worker claimed this job, and to the unique
            // `running_lane` when another worker claimed a job of its lane.
            let won = job
                .update()
                .state(JobState::Running)
                .running_lane(lane.clone())
                .attempts(attempts)
                .locked_until(Some(locked_until))
                .dedupe_key(free_key)
                .exec(&mut db)
                .await
                .is_ok();
            if won {
                busy.extend(lane);
                claimed.push(job);
            }
        }
        Ok(claimed)
    }

    /// Runs `job` and records how it ended.
    async fn execute(&self, job: QueuedJob) {
        let outcome = match self.inner.runs.get(job.kind.as_str()) {
            None => Err(topcoat::Error::msg(format!(
                "no job of kind `{}` is registered",
                job.kind
            ))),
            Some(run) => {
                let cx = self.run_context();
                tokio::time::timeout(RUN_TIMEOUT, run(&job.payload, &cx))
                    .await
                    .unwrap_or_else(|_| {
                        Err(topcoat::Error::msg(format!(
                            "timed out after {RUN_TIMEOUT:?}"
                        )))
                    })
            }
        };
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
        for mut job in lapsed {
            tracing::warn!(kind = %job.kind, id = %job.id, "a job's lock lapsed");
            let state = if job.attempts >= job.max_attempts {
                JobState::Dead
            } else {
                JobState::Queued
            };
            // A lost race means another worker recovered it first.
            let _ = job
                .update()
                .state(state)
                .running_lane(None)
                .locked_until(None)
                .last_error(Some("the worker running it stopped".to_string()))
                .exec(&mut db)
                .await;
        }
        Ok(())
    }
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
