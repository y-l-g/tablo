use std::sync::atomic::{AtomicUsize, Ordering};

use tablo_core::Panel;
use topcoat::context::context;

use super::*;
use crate::{JobSchedule, enqueue};

/// Counts its runs, and fails until the count passes `fail_until`.
#[derive(Clone, Serialize, Deserialize)]
struct Probe {
    fail_until: usize,
    lane: Option<String>,
}

const OK: Probe = Probe {
    fail_until: 0,
    lane: None,
};

#[derive(Clone, Default)]
struct Runs(Arc<AtomicUsize>);

impl Runs {
    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

impl Job for Probe {
    const KIND: &'static str = "probe";
    const MAX_ATTEMPTS: u32 = 3;

    fn lane(&self) -> Option<String> {
        self.lane.clone()
    }

    async fn run(self, cx: &Cx) -> topcoat::Result<()> {
        let run = context::<Runs>(cx).0.fetch_add(1, Ordering::SeqCst) + 1;
        if run <= self.fail_until {
            return Err(topcoat::Error::msg(format!("run {run} fails")));
        }
        Ok(())
    }
}

struct Queue {
    db: Db,
    panel: PanelHandle,
}

impl Queue {
    async fn new() -> Self {
        let db = Db::builder()
            .models(toasty::models!(QueuedJob, JobSchedule))
            .connect("sqlite::memory:")
            .await
            .expect("connect to in-memory sqlite");
        db.push_schema().await.expect("push the schema");
        let panel = Panel::new("jobs")
            .handle(&db)
            .expect("an empty panel mounts");
        Self { db, panel }
    }

    fn worker(&self, runs: &Runs) -> Worker {
        let runs = runs.clone();
        Worker::new(&self.panel)
            .expect("a handle holds its database")
            .register::<Probe>()
            .context(move |cx| cx.with(runs.clone()))
    }

    async fn enqueue(&self, job: &Probe) {
        enqueue(job)
            .exec(&mut self.db.clone())
            .await
            .expect("enqueue");
    }

    async fn rows(&self) -> Vec<QueuedJob> {
        QueuedJob::all()
            .exec(&mut self.db.clone())
            .await
            .expect("query")
    }

    /// Makes every queued job due now, as if its backoff had passed.
    async fn age(&self) {
        QueuedJob::filter(QueuedJob::fields().state().eq(JobState::Queued))
            .update()
            .run_at(ago(SignedDuration::from_secs(1)))
            .exec(&mut self.db.clone())
            .await
            .expect("update");
    }
}

fn ago(duration: SignedDuration) -> Timestamp {
    Timestamp::now().checked_sub(duration).expect("in range")
}

#[tokio::test]
async fn a_job_runs_and_is_marked_succeeded() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    queue.enqueue(&OK).await;

    queue.worker(&runs).drain().await.expect("drain");

    assert_eq!(runs.count(), 1);
    let row = queue.rows().await.remove(0);
    assert_eq!(row.state, JobState::Succeeded);
    assert_eq!(row.attempts, 1);
    assert!(row.finished_at.is_some());
}

#[tokio::test]
async fn a_rolled_back_transaction_enqueues_nothing() {
    let queue = Queue::new().await;
    let mut db = queue.db.clone();
    let mut tx = db.transaction().await.expect("begin");
    enqueue(&OK).exec(&mut tx).await.expect("enqueue");
    tx.rollback().await.expect("rollback");
    assert!(queue.rows().await.is_empty());
}

#[tokio::test]
async fn a_delay_holds_the_job_back() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    enqueue(&OK)
        .delay(Duration::from_mins(5))
        .exec(&mut queue.db.clone())
        .await
        .expect("enqueue");

    queue.worker(&runs).drain().await.expect("drain");

    assert_eq!(runs.count(), 0);
    assert_eq!(queue.rows().await[0].state, JobState::Queued);
}

#[tokio::test]
async fn a_failure_waits_its_backoff_then_succeeds() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    queue
        .enqueue(&Probe {
            fail_until: 1,
            lane: None,
        })
        .await;

    queue.worker(&runs).drain().await.expect("drain");
    let row = queue.rows().await.remove(0);
    assert_eq!(row.state, JobState::Queued);
    assert!(row.run_at > Timestamp::now(), "it waits its backoff");
    assert!(row.last_error.expect("kept").contains("run 1 fails"));

    queue.age().await;
    queue.worker(&runs).drain().await.expect("drain");
    assert_eq!(queue.rows().await[0].state, JobState::Succeeded);
}

#[tokio::test]
async fn a_job_out_of_attempts_turns_dead() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    queue
        .enqueue(&Probe {
            fail_until: 100,
            lane: None,
        })
        .await;
    for _ in 0..3 {
        queue.worker(&runs).drain().await.expect("drain");
        queue.age().await;
    }
    queue.worker(&runs).drain().await.expect("drain");

    assert_eq!(runs.count(), 3);
    assert_eq!(queue.rows().await[0].state, JobState::Dead);
}

#[tokio::test]
async fn work_arriving_during_a_run_queues_behind_it() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    let mut db = queue.db.clone();
    enqueue(&OK)
        .dedupe("key")
        .exec(&mut db)
        .await
        .expect("enqueue");
    queue.worker(&runs).drain().await.expect("drain");
    enqueue(&OK)
        .dedupe("key")
        .exec(&mut db)
        .await
        .expect("enqueue");
    assert_eq!(queue.rows().await.len(), 2, "the claim freed the key");
}

#[tokio::test]
async fn a_busy_lane_holds_its_next_job_back() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    let job = Probe {
        fail_until: 0,
        lane: Some("customer".to_string()),
    };
    queue.enqueue(&job).await;
    queue.enqueue(&job).await;

    let worker = queue.worker(&runs);
    assert_eq!(worker.drain_once().await.expect("drain"), 1);
    assert_eq!(worker.drain_once().await.expect("drain"), 1);
    assert_eq!(runs.count(), 2);
    assert!(
        queue
            .rows()
            .await
            .iter()
            .all(|row| row.running_lane.is_none()),
        "a finished run frees its lane"
    );
}

/// Holds its lane for a moment, counting how many runs of the lane overlap.
#[derive(Clone, Serialize, Deserialize)]
struct Hold;

#[derive(Default)]
struct Overlap {
    running: AtomicUsize,
    most: AtomicUsize,
}

impl Job for Hold {
    const KIND: &'static str = "hold";

    fn lane(&self) -> Option<String> {
        Some("customer".to_string())
    }

    async fn run(self, cx: &Cx) -> topcoat::Result<()> {
        let overlap = context::<Arc<Overlap>>(cx);
        let running = overlap.running.fetch_add(1, Ordering::SeqCst) + 1;
        overlap.most.fetch_max(running, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        overlap.running.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn racing_workers_never_overlap_a_lane() {
    let queue = Queue::new().await;
    let overlap = Arc::new(Overlap::default());
    for _ in 0..4 {
        enqueue(&Hold)
            .exec(&mut queue.db.clone())
            .await
            .expect("enqueue");
    }
    let racers: Vec<_> = (0..4)
        .map(|_| {
            let overlap = Arc::clone(&overlap);
            Worker::new(&queue.panel)
                .expect("a handle holds its database")
                .register::<Hold>()
                .context(move |cx| cx.with(Arc::clone(&overlap)))
        })
        .map(|worker| tokio::spawn(async move { worker.drain_once().await }))
        .collect();
    let mut claimed = 0;
    for racer in racers {
        claimed += racer.await.expect("join").expect("drain");
    }
    assert_eq!(overlap.most.load(Ordering::SeqCst), 1);
    assert_eq!(claimed, 1, "the lane held the other three back");
}

#[tokio::test]
async fn racing_workers_run_each_job_once() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    for _ in 0..8 {
        queue.enqueue(&OK).await;
    }
    let racers: Vec<_> = (0..4)
        .map(|_| queue.worker(&runs))
        .map(|worker| tokio::spawn(async move { worker.drain_once().await }))
        .collect();
    for racer in racers {
        racer.await.expect("join").expect("drain");
    }
    assert_eq!(runs.count(), 8);
}

#[tokio::test]
async fn a_lapsed_lock_gives_the_job_to_another_worker() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    queue
        .enqueue(&Probe {
            fail_until: 0,
            lane: Some("customer".to_string()),
        })
        .await;
    let past = ago(SignedDuration::from_mins(1));
    QueuedJob::all()
        .update()
        .state(JobState::Running)
        .running_lane(Some("customer".to_string()))
        .attempts(1)
        .locked_until(Some(past))
        .run_at(past)
        .exec(&mut queue.db.clone())
        .await
        .expect("a worker took it and stopped");

    let worker = queue.worker(&runs);
    worker.drain().await.expect("drain");
    let row = queue.rows().await.remove(0);
    assert_eq!(runs.count(), 0, "a recovered job waits its backoff");
    assert_eq!(row.state, JobState::Queued);
    assert!(row.running_lane.is_none(), "recovery frees the lane");

    queue.age().await;
    worker.drain().await.expect("drain");
    assert_eq!(runs.count(), 1);
    let row = queue.rows().await.remove(0);
    assert_eq!(row.state, JobState::Succeeded);
    assert_eq!(row.attempts, 2);
}

#[tokio::test]
async fn a_lapsed_lock_out_of_attempts_turns_dead_and_finished() {
    let queue = Queue::new().await;
    queue.enqueue(&OK).await;
    let past = ago(SignedDuration::from_mins(1));
    QueuedJob::all()
        .update()
        .state(JobState::Running)
        .attempts(u64::from(Probe::MAX_ATTEMPTS))
        .locked_until(Some(past))
        .exec(&mut queue.db.clone())
        .await
        .expect("a worker took its last attempt and stopped");

    queue.worker(&Runs::default()).drain().await.expect("drain");

    let row = queue.rows().await.remove(0);
    assert_eq!(row.state, JobState::Dead);
    assert!(row.finished_at.is_some());
}

#[tokio::test]
async fn a_worker_leaves_the_kinds_it_does_not_register() {
    let queue = Queue::new().await;
    queue.enqueue(&OK).await;

    let claimed = Worker::new(&queue.panel)
        .expect("a handle holds its database")
        .drain_once()
        .await
        .expect("drain");

    assert_eq!(claimed, 0);
    let row = queue.rows().await.remove(0);
    assert_eq!(row.state, JobState::Queued);
    assert_eq!(row.attempts, 0, "no attempt spent");

    let runs = Runs::default();
    queue.worker(&runs).drain().await.expect("drain");
    assert_eq!(runs.count(), 1, "a worker that registers it runs it");
}

#[tokio::test]
async fn a_schedule_enqueues_once_per_period_across_workers() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    let scheduled = || queue.worker(&runs).every(Duration::from_hours(1), OK);

    scheduled().drain().await.expect("drain");
    assert_eq!(runs.count(), 0, "the first run is one interval away");

    JobSchedule::filter_by_kind(Probe::KIND)
        .update()
        .next_run_at(ago(SignedDuration::from_secs(1)))
        .exec(&mut queue.db.clone())
        .await
        .expect("fall due");
    scheduled().drain().await.expect("drain");
    scheduled().drain().await.expect("drain");

    assert_eq!(runs.count(), 1);
}

#[tokio::test]
async fn the_prune_deletes_old_successes_and_keeps_the_dead() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    for _ in 0..3 {
        queue.enqueue(&OK).await;
    }
    let rows = queue.rows().await;
    let finished = ago(SignedDuration::from_hours(48));
    for (row, state) in rows.iter().zip([JobState::Succeeded, JobState::Dead]) {
        QueuedJob::filter_by_id(row.id)
            .update()
            .state(state)
            .finished_at(Some(finished))
            .exec(&mut queue.db.clone())
            .await
            .expect("finish");
    }
    let worker = queue.worker(&runs).keep_succeeded(Duration::from_hours(24));
    worker.drain().await.expect("seen the schedule");
    JobSchedule::filter_by_kind(PruneJobs::KIND)
        .update()
        .next_run_at(ago(SignedDuration::from_secs(1)))
        .exec(&mut queue.db.clone())
        .await
        .expect("fall due");

    worker.drain().await.expect("drain");

    let left: Vec<_> = queue
        .rows()
        .await
        .into_iter()
        .filter(|row| row.kind == Probe::KIND)
        .collect();
    assert_eq!(left.len(), 2);
    assert!(
        left.iter().all(|row| row.id != rows[0].id),
        "the old success is gone"
    );
    assert!(left.iter().any(|row| row.state == JobState::Dead));
    assert_eq!(runs.count(), 1, "the fresh success stays");
}

#[test]
fn backoff_doubles_up_to_its_cap() {
    let id = uuid::Uuid::nil();
    assert_eq!(backoff(1, id).as_secs(), 30);
    assert_eq!(backoff(2, id).as_secs(), 60);
    assert_eq!(backoff(3, id).as_secs(), 120);
    assert_eq!(backoff(40, id).as_secs(), BACKOFF_CAP_SECONDS);
}

#[test]
fn jitter_adds_at_most_a_quarter() {
    let id = uuid::Uuid::from_u128(u128::MAX);
    assert_eq!(backoff(1, id).as_secs(), 30 + 7);
    assert_eq!(backoff(40, id).as_secs(), BACKOFF_CAP_SECONDS * 5 / 4);
}

#[tokio::test]
async fn a_busy_lane_does_not_hold_back_other_jobs() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    let laned = Probe {
        fail_until: 0,
        lane: Some("customer".to_string()),
    };
    for _ in 0..BATCH + 4 {
        queue.enqueue(&laned).await;
    }
    // One job of the lane runs on a worker elsewhere.
    let mut running = queue.rows().await.remove(0);
    running
        .update()
        .state(JobState::Running)
        .running_lane(Some("customer".to_string()))
        .locked_until(Some(
            Timestamp::now().checked_add(LOCK_MARGIN).expect("in range"),
        ))
        .exec(&mut queue.db.clone())
        .await
        .expect("claim elsewhere");
    queue.enqueue(&OK).await;

    let claimed = queue.worker(&runs).drain_once().await.expect("drain");

    assert_eq!(claimed, 1, "the free job behind the lane's backlog");
    assert_eq!(runs.count(), 1);
}

/// Runs until its context's gate opens.
#[derive(Clone, Serialize, Deserialize)]
struct Gate;

impl Job for Gate {
    const KIND: &'static str = "gate";

    async fn run(self, cx: &Cx) -> topcoat::Result<()> {
        let mut open = context::<watch::Receiver<bool>>(cx).clone();
        open.wait_for(|open| *open)
            .await
            .map_err(|_| topcoat::Error::msg("the gate dropped"))?;
        Ok(())
    }
}

#[tokio::test]
async fn a_long_run_leaves_the_worker_free_for_the_next_job() {
    let queue = Queue::new().await;
    let runs = Runs::default();
    let (open, gate) = watch::channel(false);
    enqueue(&Gate)
        .exec(&mut queue.db.clone())
        .await
        .expect("enqueue");
    let worker = queue.worker(&runs).register::<Gate>().context({
        let runs = runs.clone();
        move |cx| cx.with(runs.clone()).with(gate.clone())
    });
    let (stop, stopped) = watch::channel(false);
    let running = tokio::spawn(worker.run(stopped));

    queue.enqueue(&OK).await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while runs.count() == 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the job behind the long run ran");

    open.send(true).expect("open the gate");
    stop.send(true).expect("stop");
    running.await.expect("join");
    assert!(
        queue
            .rows()
            .await
            .iter()
            .all(|row| row.state == JobState::Succeeded),
        "shutting down lets the long run finish"
    );
}

/// Outlasts its own timeout.
#[derive(Clone, Serialize, Deserialize)]
struct Slow;

impl Job for Slow {
    const KIND: &'static str = "slow";
    const TIMEOUT: Duration = Duration::from_millis(20);

    async fn run(self, _: &Cx) -> topcoat::Result<()> {
        tokio::time::sleep(Duration::from_secs(5)).await;
        Ok(())
    }
}

#[tokio::test]
async fn a_run_past_its_kinds_timeout_fails() {
    let queue = Queue::new().await;
    enqueue(&Slow)
        .exec(&mut queue.db.clone())
        .await
        .expect("enqueue");

    Worker::new(&queue.panel)
        .expect("a handle holds its database")
        .register::<Slow>()
        .drain_once()
        .await
        .expect("drain");

    let row = queue.rows().await.remove(0);
    assert_eq!(row.state, JobState::Queued);
    assert!(row.last_error.expect("kept").contains("timed out"));
}

#[tokio::test]
async fn a_dedupe_key_is_scoped_to_its_kind() {
    let queue = Queue::new().await;
    let mut db = queue.db.clone();
    enqueue(&OK)
        .dedupe("shared")
        .exec(&mut db)
        .await
        .expect("enqueue");
    enqueue(&Slow)
        .dedupe("shared")
        .exec(&mut db)
        .await
        .expect("enqueue");
    assert_eq!(queue.rows().await.len(), 2);
}
