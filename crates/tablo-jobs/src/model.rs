//! The queue's tables: a [`QueuedJob`] per unit of work and a [`JobSchedule`] per recurring job.

use jiff::Timestamp;
use uuid::Uuid;

/// One unit of background work, written by [`enqueue`](crate::enqueue) and run by a
/// [`Worker`](crate::Worker).
///
/// It is a [`PanelModel`](tablo_core::PanelModel), so an app can list the queue through a
/// resource of its own, and give a [`Dead`](JobState::Dead) job its attempts back by setting its
/// state to [`Queued`](JobState::Queued), `attempts` to 0 and `run_at` to now.
#[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
#[table = "tablo_jobs"]
pub struct QueuedJob {
    #[key]
    pub id: Uuid,
    /// The [`Job::KIND`](crate::Job::KIND) that runs it.
    #[index]
    pub kind: String,
    /// The job's fields, as JSON.
    #[column(type = text)]
    #[columns(skip)]
    pub payload: String,
    pub state: JobState,
    /// The [`Job::lane`](crate::Job::lane) it runs in: one job of a lane runs at a time.
    #[index]
    pub lane: Option<String>,
    /// The lane while the job runs, and `None` otherwise. Unique, so the database refuses a
    /// second running job in one lane, whichever worker claims it.
    #[unique]
    #[columns(skip)]
    pub running_lane: Option<String>,
    /// While the job waits, a second enqueue with the same key adds nothing. A claim frees it,
    /// so work arriving during a run queues behind that run.
    #[unique]
    #[columns(skip)]
    pub dedupe_key: String,
    pub attempts: u64,
    pub max_attempts: u64,
    /// Not before this instant.
    #[index]
    pub run_at: Timestamp,
    /// When a running job's worker gives it up, alive or not.
    pub locked_until: Option<Timestamp>,
    /// The last failure, for whoever reads a dead job.
    #[column(type = text)]
    pub last_error: Option<String>,
    pub created_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    /// Makes a claim a compare-and-swap: of two workers, the second fails its update.
    #[version]
    pub version: u64,
}

/// Where a [`QueuedJob`] stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo_core::Options)]
pub enum JobState {
    /// Waiting for its `run_at`, or for its lane.
    #[option(color = "gray")]
    Queued,
    /// Held by a worker until its `locked_until`.
    #[option(color = "info")]
    Running,
    #[option(color = "success")]
    Succeeded,
    /// Out of attempts: it runs again only when someone queues it again.
    #[option(color = "danger")]
    Dead,
}

/// When a recurring job next falls due: one row per schedule, shared by every worker.
#[derive(Debug, Clone, toasty::Model)]
#[table = "tablo_job_schedules"]
pub struct JobSchedule {
    /// The [`Job::KIND`](crate::Job::KIND) it enqueues.
    #[key]
    pub kind: String,
    pub next_run_at: Timestamp,
    #[version]
    pub version: u64,
}
