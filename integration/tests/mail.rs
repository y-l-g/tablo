//! Auth mail: the mistakes mounting refuses in what a panel's auth mails, and the delivery that
//! takes each letter inside the transaction recording its token.

use tablo::{
    Auth, AuthLetter, AuthLetterKind, AuthMail, AuthMailDelivery, AuthMailFault,
    DeclarationErrorKind, NoForm, Panel, PasswordAuth, PasswordReset, ReadOnly, Resource,
    ResourceDef, RouterBuilderPanelExt, Table, TextColumn,
    auth::{AdminUser, AuthSession, AuthToken, create_admin, verify_password},
    lens,
    testing::form_body,
};
use toasty::Db;
use topcoat::{
    context::Cx,
    mail::{Mailbox, Mailer, MemoryTransport},
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use crate::common::{
    TestClient,
    mail::{BASE_URL, link, token},
    refusal, test_db,
};

#[derive(Debug, Clone, toasty::Model, tablo::PanelModel)]
struct Memo {
    #[key]
    #[auto]
    id: Uuid,
    body: String,
}

struct MemoResource;

impl Resource for MemoResource {
    type Model = Memo;
    type Form = NoForm<Memo>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Memo.body))))
    }
}

/// A letter an [`Outbox`] queued, as JSON.
#[derive(Debug, Clone, toasty::Model)]
struct QueuedLetter {
    #[key]
    #[auto]
    id: Uuid,
    json: String,
}

async fn db() -> Db {
    test_db(toasty::models!(
        Memo,
        AdminUser,
        AuthSession,
        AuthToken,
        QueuedLetter
    ))
    .await
}

/// Queues each letter through the transaction it is handed.
struct Outbox;

impl AuthMailDelivery for Outbox {
    async fn deliver(
        &self,
        _cx: &Cx,
        ex: &mut dyn toasty::Executor,
        letter: AuthLetter,
    ) -> topcoat::Result<()> {
        let json = serde_json::to_string(&letter)?;
        toasty::create!(QueuedLetter { json }).exec(ex).await?;
        Ok(())
    }
}

/// Refuses every letter, as a queue whose table is gone would.
struct Refusing;

impl AuthMailDelivery for Refusing {
    async fn deliver(
        &self,
        _cx: &Cx,
        _ex: &mut dyn toasty::Executor,
        _letter: AuthLetter,
    ) -> topcoat::Result<()> {
        Err(topcoat::Error::msg("the queue is down"))
    }
}

/// A panel resetting passwords whose letters go to `delivery`, on a router without a `Mailer`.
async fn delivering(db: &Db, delivery: impl AuthMailDelivery) -> Router {
    let mail = AuthMail::new(sender(), BASE_URL).delivery(delivery);
    let auth = Auth::password()
        .mail(mail)
        .password_reset(PasswordReset::new(PasswordAuth::new()));
    let mut builder = Router::builder().discover().with(db.clone());
    builder
        .panel(Panel::new("admin").auth(auth).resource::<MemoResource>())
        .expect("a panel with a delivery mounts without a Mailer");
    builder.build()
}

async fn queued(db: &Db) -> Vec<AuthLetter> {
    QueuedLetter::all()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .into_iter()
        .map(|row| serde_json::from_str(&row.json).unwrap())
        .collect()
}

fn sender() -> Mailbox {
    Mailbox::new("no-reply@example.com").unwrap()
}

/// Mounts `auth` on a router that can send mail when `with_mail`.
async fn mount(auth: Auth, with_mail: bool) -> topcoat::Result<Router> {
    let mut builder = Router::builder().discover().with(db().await);
    if with_mail {
        builder = builder.with(Mailer::new(MemoryTransport::new()));
    }
    let panel = Panel::new("admin").auth(auth).resource::<MemoResource>();
    builder.panel(panel)?;
    Ok(builder.build())
}

/// The faults mounting `auth` reports.
async fn faults(auth: Auth, with_mail: bool) -> Vec<DeclarationErrorKind> {
    refusal(mount(auth, with_mail).await)
        .into_iter()
        .map(|error| error.kind)
        .collect()
}

/// A feature that mails links needs a sender, a base URL the link can start with, and a router
/// that sends mail.
#[tokio::test]
async fn mounting_refuses_mail_the_panel_cannot_send() {
    let reset = || PasswordReset::new(PasswordAuth::new());

    assert_eq!(
        faults(Auth::password().password_reset(reset()), true).await,
        vec![DeclarationErrorKind::AuthMail(AuthMailFault::Unset {
            feature: "password_reset"
        })]
    );

    let relative = AuthMail::new(sender(), "admin.example.com");
    assert_eq!(
        faults(
            Auth::password().mail(relative).password_reset(reset()),
            true
        )
        .await,
        vec![DeclarationErrorKind::AuthMail(AuthMailFault::BaseUrl(
            "admin.example.com".to_string()
        ))]
    );

    let mail = AuthMail::new(sender(), "https://admin.example.com");
    assert_eq!(
        faults(Auth::password().mail(mail).password_reset(reset()), false).await,
        vec![DeclarationErrorKind::AuthMail(AuthMailFault::MissingMail)]
    );
}

/// A sender with nothing to mail needs no mail configuration.
#[tokio::test]
async fn a_sender_without_a_feature_that_mails_mounts_anywhere() {
    let mail = AuthMail::new(sender(), "not a url");
    assert!(mount(Auth::password().mail(mail), false).await.is_ok());
}

/// A delivery queues the reset letter in place of sending it, and the link it carries works once
/// a worker sends it: the token committed with the letter.
#[tokio::test]
async fn a_delivery_queues_the_letter_with_its_token() {
    let db = db().await;
    let admin = create_admin(&mut db.clone(), "ada@example.com", "old password", "Ada")
        .await
        .unwrap();
    let router = delivering(&db, Outbox).await;

    TestClient::new(&router)
        .submit(
            "/admin/forgot-password",
            &form_body(&[("email", "ada@example.com")]),
        )
        .await;

    let letters = queued(&db).await;
    assert_eq!(letters.len(), 1);
    let letter = &letters[0];
    assert_eq!(letter.kind(), AuthLetterKind::PasswordReset);
    assert_eq!(letter.to(), "ada@example.com");
    assert!(
        letter
            .expires_at()
            .is_some_and(|at| at > jiff::Timestamp::now())
    );

    let sent = letter.mail().unwrap();
    let password = "a brand new password";
    let response = TestClient::new(&router)
        .submit(
            "/admin/reset-password",
            &form_body(&[
                ("token", &token(&link(&sent))),
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

/// A delivery that refuses the letter rolls back the token it would carry: no link works that no
/// mail carries.
#[tokio::test]
async fn a_refused_letter_records_no_token() {
    let db = db().await;
    create_admin(&mut db.clone(), "ada@example.com", "old password", "Ada")
        .await
        .unwrap();
    let router = delivering(&db, Refusing).await;

    let response = TestClient::new(&router)
        .submit(
            "/admin/forgot-password",
            &form_body(&[("email", "ada@example.com")]),
        )
        .await;

    assert_eq!(
        response.status(),
        303,
        "the answer never says whether it went out"
    );
    assert!(
        AuthToken::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_empty()
    );
}
