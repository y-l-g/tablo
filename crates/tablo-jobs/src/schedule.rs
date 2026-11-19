//! Recurring jobs. A [`JobSchedule`] row holds when each next falls due, and its version check
//! lets one of however many workers enqueue each run.

use std::{sync::Arc, time::Duration};

use jiff::{SignedDuration, Timestamp};
use toasty::Db;

use crate::{Job, JobSchedule, enqueue, job::BoxFuture};

type Enqueuer = Arc<
    dyn for<'a> Fn(&'a mut dyn toasty::Executor) -> BoxFuture<'a, topcoat::Result<()>>
        + Send
        + Sync,
>;

/// A job a worker enqueues every so often, from [`Worker::every`](crate::Worker::every).
#[derive(Clone)]
pub(crate) struct Schedule {
    kind: &'static str,
    every: SignedDuration,
    enqueue: Enqueuer,
}

impl Schedule {
    pub(crate) fn new<J: Job + Clone>(every: Duration, job: J) -> Self {
        Self {
            kind: J::KIND,
            // Past `SignedDuration::MAX`, some 292 billion years, is never.
            every: SignedDuration::try_from(every).unwrap_or(SignedDuration::MAX),
            enqueue: Arc::new(move |ex| {
                let job = job.clone();
                // Keyed by kind, so a run still waiting absorbs the next.
                Box::pin(async move { enqueue(&job).dedupe(J::KIND).exec(ex).await })
            }),
        }
    }

    pub(crate) fn kind(&self) -> &'static str {
        self.kind
    }

    /// Enqueues the run when one is due and no other worker took it first. A schedule seen for
    /// the first time falls due one interval later.
    pub(crate) async fn tick(&self, db: &Db) -> topcoat::Result<()> {
        let mut db = db.clone();
        let now = Timestamp::now();
        let next = now.saturating_add(self.every)?;
        let Some(mut row) = JobSchedule::filter_by_kind(self.kind)
            .first()
            .exec(&mut db)
            .await?
        else {
            JobSchedule::upsert_by_kind(self.kind)
                .on_create(|create| create.next_run_at(next))
                .or_ignore()
                .exec(&mut db)
                .await?;
            return Ok(());
        };
        if row.next_run_at > now {
            return Ok(());
        }
        let mut tx = db.transaction().await?;
        if row.update().next_run_at(next).exec(&mut tx).await.is_err() {
            // Another worker took this run.
            return Ok(());
        }
        (self.enqueue)(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
