//! Putting work on the queue.

use std::time::Duration;

use jiff::{SignedDuration, Timestamp};

use crate::{Job, JobState, QueuedJob};

/// A job on its way to the queue, from [`enqueue`]: set its delay or its dedupe key, then
/// [`exec`](Self::exec) it.
#[must_use = "an `Enqueue` writes nothing until `exec`"]
pub struct Enqueue<'a, J> {
    job: &'a J,
    delay: Duration,
    dedupe_key: Option<String>,
}

/// Queues `job`, once [`exec`](Enqueue::exec) writes it.
///
/// ```rust,no_run
/// # use serde::{Deserialize, Serialize};
/// # use tablo_jobs::{Job, enqueue};
/// # #[derive(Serialize, Deserialize)]
/// # struct SyncSeats { team: u64 }
/// # impl Job for SyncSeats {
/// #     const KIND: &'static str = "sync-seats";
/// #     async fn run(self, _: &topcoat::context::Cx) -> topcoat::Result<()> { Ok(()) }
/// # }
/// # async fn seat_added(tx: &mut dyn toasty::Executor) -> topcoat::Result<()> {
/// enqueue(&SyncSeats { team: 7 })
///     .dedupe("sync-seats:7")
///     .exec(tx)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub fn enqueue<J: Job>(job: &J) -> Enqueue<'_, J> {
    Enqueue {
        job,
        delay: Duration::ZERO,
        dedupe_key: None,
    }
}

impl<J: Job> Enqueue<'_, J> {
    /// Runs it no sooner than `delay` from now.
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// While a job with `key` waits, adds nothing: a burst of ten enqueues makes one run. Once a
    /// worker claims that job, the key is free again, so work arriving during the run queues
    /// behind it.
    pub fn dedupe(mut self, key: impl Into<String>) -> Self {
        self.dedupe_key = Some(key.into());
        self
    }

    /// Writes the job through `ex`. Pass the transaction of the write that implies the job, so
    /// the two commit together or not at all.
    ///
    /// # Errors
    ///
    /// When the job does not serialize, or the driver fails to write it.
    pub async fn exec(self, ex: &mut dyn toasty::Executor) -> topcoat::Result<()> {
        let id = uuid::Uuid::now_v7();
        let now = Timestamp::now();
        let run_at = now.checked_add(SignedDuration::try_from(self.delay)?)?;
        let payload = serde_json::to_string(self.job)?;
        let create = |key: String| {
            QueuedJob::create()
                .id(id)
                .kind(J::KIND.to_string())
                .payload(payload.clone())
                .state(JobState::Queued)
                .lane(self.job.lane())
                .dedupe_key(key)
                .attempts(0)
                .max_attempts(u64::from(J::MAX_ATTEMPTS))
                .run_at(run_at)
                .created_at(now)
        };
        let Some(key) = self.dedupe_key.clone() else {
            // A fresh key meets no other: a plain insert, on every driver.
            create(id.to_string()).exec(ex).await?;
            return Ok(());
        };
        let upserted = QueuedJob::upsert_by_dedupe_key(key.clone())
            .on_create(|row| {
                row.id(id)
                    .kind(J::KIND.to_string())
                    .payload(payload.clone())
                    .state(JobState::Queued)
                    .lane(self.job.lane())
                    .attempts(0)
                    .max_attempts(u64::from(J::MAX_ATTEMPTS))
                    .run_at(run_at)
                    .created_at(now)
            })
            .or_ignore()
            .exec(ex)
            .await;
        match upserted {
            Ok(_) => Ok(()),
            // MySQL has no targeted upsert, and Toasty refuses it before any SQL runs.
            Err(error) if error.is_unsupported_feature() => {
                if waiting(ex, &key).await? {
                    return Ok(());
                }
                match create(key.clone()).exec(ex).await {
                    Ok(_) => Ok(()),
                    // A racing enqueue wrote the key first. MySQL rolls back the statement, not
                    // the transaction, so the look-up still runs.
                    Err(_) if waiting(ex, &key).await? => Ok(()),
                    Err(error) => Err(error.into()),
                }
            }
            Err(error) => Err(error.into()),
        }
    }
}

/// Whether a job holding `key` waits in the queue.
async fn waiting(ex: &mut dyn toasty::Executor, key: &str) -> topcoat::Result<bool> {
    Ok(QueuedJob::filter_by_dedupe_key(key)
        .first()
        .exec(ex)
        .await?
        .is_some())
}
