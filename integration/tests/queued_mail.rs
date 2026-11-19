//! `tablo::jobs::AuthMailQueue`: a panel's auth letter queued sealed with its token, and sent by
//! the worker.

use tablo::{
    Auth, AuthMail, NoForm, Panel, PanelHandle, PasswordAuth, PasswordReset, ReadOnly, Resource,
    ResourceDef, RouterBuilderPanelExt, Table, TextColumn,
    auth::{AdminUser, AuthSession, AuthToken, create_admin, verify_password},
    jobs::{AuthMailQueue, JobSchedule, JobState, QueuedJob, Worker},
    lens,
    testing::form_body,
};
use toasty::Db;
use topcoat::{
    cookie::Key,
    mail::{Mailbox, Mailer, MemoryTransport},
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use crate::common::{
    TestClient,
    mail::{BASE_URL, link, token},
    test_db,
};

#[derive(Debug, Clone, toasty::Model, tablo::PanelModel)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    body: String,
}

struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = NoForm<Note>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Note.body))))
    }
}

/// A router whose panel queues its reset letters, without a `Mailer`, and the panel's handle.
async fn queueing(db: &Db, queue: &AuthMailQueue) -> (Router, PanelHandle) {
    let auth = Auth::password()
        .mail(
            AuthMail::new(Mailbox::new("no-reply@example.com").unwrap(), BASE_URL)
                .delivery(queue.clone()),
        )
        .password_reset(PasswordReset::new(PasswordAuth::new()));
    let mut builder = Router::builder().discover().with(db.clone());
    let panel = builder
        .panel(Panel::new("admin").auth(auth).resource::<NoteResource>())
        .expect("a panel with a queue mounts without a Mailer");
    (builder.build(), panel)
}

/// A reset letter waits in the queue, sealed, until the worker sends it, and its link works.
#[tokio::test]
async fn a_queued_reset_letter_is_sealed_and_sent_by_the_worker() {
    let db = test_db(toasty::models!(
        Note,
        AdminUser,
        AuthSession,
        AuthToken,
        QueuedJob,
        JobSchedule
    ))
    .await;
    let admin = create_admin(&mut db.clone(), "ada@example.com", "old password", "Ada")
        .await
        .unwrap();
    let queue = AuthMailQueue::new(&Key::generate());
    let (router, panel) = queueing(&db, &queue).await;
    let sent = MemoryTransport::new();
    let transport = sent.clone();
    let worker = Worker::new(&panel)
        .unwrap()
        .auth_mail(&queue)
        .context(move |cx| cx.with(Mailer::new(transport.clone())));

    TestClient::new(&router)
        .submit(
            "/admin/forgot-password",
            &form_body(&[("email", "ada@example.com")]),
        )
        .await;

    let jobs = QueuedJob::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert!(
        !jobs[0].payload.contains("ada@example.com"),
        "the letter is sealed"
    );
    assert!(sent.sent().is_empty());

    worker.drain().await.unwrap();

    let mail = sent.sent();
    assert_eq!(mail.len(), 1);
    let job = QueuedJob::filter_by_id(jobs[0].id)
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(job.state, JobState::Succeeded);

    let password = "a brand new password";
    let response = TestClient::new(&router)
        .submit(
            "/admin/reset-password",
            &form_body(&[
                ("token", &token(&link(&mail[0]))),
                ("password", password),
                ("password_confirmation", password),
            ]),
        )
        .await;
    assert_eq!(response.status(), 303);
    let stored = AdminUser::filter(AdminUser::fields().id().eq(admin.id))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .unwrap();
    assert!(verify_password(password, Some(&stored.password_hash)).await);
}
