use tablo_core::Panel;
use topcoat::mail::{Mailer, MemoryTransport};

use super::*;
use crate::{JobSchedule, JobState, QueuedJob, Worker};

/// A letter as the queue stores it, before sealing. Built through its serialized form, since
/// only a panel's auth writes one.
fn letter(expires_at: Option<Timestamp>) -> AuthLetter {
    serde_json::from_value(serde_json::json!({
        "kind": "password_reset",
        "from_name": null,
        "from": "no-reply@example.com",
        "to": "ada@example.com",
        "subject": "Reset your password",
        "text": "https://app.example.com/reset?token=secret-token",
        "date": Timestamp::now(),
        "message_id": "<letter@example.com>",
        "expires_at": expires_at,
    }))
    .expect("an AuthLetter's serialized form")
}

#[test]
fn a_sealed_letter_opens_under_its_key_alone() {
    let key = Key::generate();
    let queue = AuthMailQueue::new(&key);
    let sealed = queue.seal(b"token=secret").expect("seal");

    assert_eq!(queue.open(&sealed).expect("open"), b"token=secret");
    assert!(AuthMailQueue::new(&Key::generate()).open(&sealed).is_err());
    assert_ne!(queue.seal(b"token=secret").expect("seal"), sealed);

    let mut tampered = BASE64URL_NOPAD.decode(sealed.as_bytes()).expect("base64");
    *tampered.last_mut().expect("bytes") ^= 1;
    assert!(queue.open(&BASE64URL_NOPAD.encode(&tampered)).is_err());
    assert!(queue.open("short").is_err());
}

struct Harness {
    db: toasty::Db,
    queue: AuthMailQueue,
    worker: Worker,
    sent: MemoryTransport,
}

async fn harness(key: &Key, worker_key: &Key) -> Harness {
    let db = toasty::Db::builder()
        .models(toasty::models!(QueuedJob, JobSchedule))
        .connect("sqlite::memory:")
        .await
        .expect("connect to in-memory sqlite");
    db.push_schema().await.expect("push the schema");
    let panel = Panel::new("app")
        .handle(&db)
        .expect("an empty panel mounts");
    let sent = MemoryTransport::new();
    let transport = sent.clone();
    let worker = Worker::new(&panel)
        .expect("a handle holds its database")
        .auth_mail(&AuthMailQueue::new(worker_key))
        .context(move |cx| cx.with(Mailer::new(transport.clone())));
    Harness {
        db,
        queue: AuthMailQueue::new(key),
        worker,
        sent,
    }
}

impl Harness {
    async fn deliver(&self, letter: AuthLetter) {
        AuthMailDelivery::deliver(&self.queue, &Cx::new(), &mut self.db.clone(), letter)
            .await
            .expect("queue the letter");
    }

    async fn job(&self) -> QueuedJob {
        QueuedJob::filter_by_kind(SendAuthMail::KIND)
            .first()
            .exec(&mut self.db.clone())
            .await
            .expect("query")
            .expect("the letter's job")
    }
}

#[tokio::test]
async fn a_queued_letter_is_stored_sealed_and_sent_by_the_worker() {
    let key = Key::generate();
    let harness = harness(&key, &key).await;
    let live = Timestamp::now()
        .checked_add(jiff::SignedDuration::from_hours(1))
        .expect("in range");

    harness.deliver(letter(Some(live))).await;
    let job = harness.job().await;
    assert!(!job.payload.contains("secret-token"));
    assert!(!job.payload.contains("ada@example.com"));
    assert!(
        harness.sent.sent().is_empty(),
        "nothing goes out before the worker runs"
    );

    harness.worker.drain().await.expect("drain");

    let sent = harness.sent.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].message_id(), "<letter@example.com>");
    assert_eq!(harness.job().await.state, JobState::Succeeded);
}

#[tokio::test]
async fn an_expired_letter_is_dropped_unsent() {
    let key = Key::generate();
    let harness = harness(&key, &key).await;
    let past = Timestamp::now()
        .checked_sub(jiff::SignedDuration::from_secs(1))
        .expect("in range");

    harness.deliver(letter(Some(past))).await;
    harness.worker.drain().await.expect("drain");

    assert!(harness.sent.sent().is_empty());
    assert_eq!(harness.job().await.state, JobState::Succeeded);
}

#[tokio::test]
async fn a_worker_under_another_key_sends_nothing() {
    let harness = harness(&Key::generate(), &Key::generate()).await;

    harness.deliver(letter(None)).await;
    harness.worker.drain().await.expect("drain");

    assert!(harness.sent.sent().is_empty());
    let job = harness.job().await;
    assert_eq!(job.state, JobState::Queued, "it retries, then dies");
    assert!(job.last_error.expect("kept").contains("does not open"));
}
