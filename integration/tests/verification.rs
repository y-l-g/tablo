//! Email verification: the link a sign-up mails, what an unverified user reaches, the links it
//! refuses, and the mistakes mounting refuses.

use std::time::Duration;

use http::header::{CACHE_CONTROL, LOCATION};
use jiff::Timestamp;
use tablo::{
    Auth, AuthMail, AuthMailFault, DeclarationErrorKind, EmailVerification, LoginThrottle, NoForm,
    NotificationStatus, Panel, PasswordAuth, PasswordSignUp, ReadOnly, Resource, ResourceDef,
    RouterBuilderPanelExt, Table, TextColumn, VerificationFault,
    auth::{AdminUser, AuthSession, AuthToken, create_admin},
    lens,
    testing::{SESSION_COOKIE, form_body, session_cookie_value},
};
use toasty::Db;
use topcoat::{
    mail::{MailConfig, Mailbox, MemoryTransport, RouterBuilderMailExt},
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use crate::common::{
    TestClient, flash,
    mail::{BASE_URL, link, mails},
    refusal, test_db,
};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Ticket {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

struct TicketResource;

impl Resource for TicketResource {
    type Model = Ticket;
    type Form = NoForm<Ticket>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Ticket.title))))
    }
}

async fn db() -> Db {
    test_db(toasty::models!(Ticket, AdminUser, AuthSession, AuthToken)).await
}

/// The shipped authenticator, mailing from a test sender.
fn mailing() -> Auth {
    Auth::password().mail(AuthMail::new(
        Mailbox::new("no-reply@example.com").unwrap(),
        BASE_URL,
    ))
}

/// A panel with sign-up and email verification, mounting `verification`.
fn verifying(verification: EmailVerification<AdminUser>) -> Auth {
    mailing()
        .registration(PasswordSignUp::open())
        .email_verification(verification)
}

fn mount(db: Db, auth: Auth, mail: &MemoryTransport) -> topcoat::Result<Router> {
    Ok(Router::builder()
        .discover()
        .with(db)
        .mail(MailConfig::builder().transport(mail.clone()).build())
        .panel(Panel::new("admin").auth(auth).resource::<TicketResource>())?
        .build())
}

fn router(db: Db, mail: &MemoryTransport) -> Router {
    mount(db, verifying(EmailVerification::new(PasswordAuth)), mail).expect("panel builds")
}

async fn admin(db: &Db, email: &str) -> AdminUser {
    AdminUser::filter(AdminUser::fields().email().eq(email.to_string()))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the admin exists")
}

/// An admin who signs in with `email` and has not verified it.
async fn unverified(db: &Db, email: &str) -> AdminUser {
    let admin = create_admin(&mut db.clone(), email, "a password", "Ada")
        .await
        .unwrap();
    AdminUser::filter(AdminUser::fields().id().eq(admin.id))
        .update()
        .email_verified_at(None)
        .exec(&mut db.clone())
        .await
        .unwrap();
    self::admin(db, email).await
}

async fn sign_up(router: &Router, email: &str) -> http::Response<topcoat::router::Body> {
    TestClient::new(router)
        .submit(
            "/admin/register",
            &form_body(&[
                ("name", "Ada"),
                ("email", email),
                ("password", "a password"),
                ("password_confirmation", "a password"),
            ]),
        )
        .await
}

/// A sign-up mails a link to the new address and signs the account in, which then reaches only
/// the page asking for the proof until the link verifies it; a used link works no more.
#[tokio::test]
async fn a_sign_up_reaches_the_panel_once_its_link_verifies_the_address() {
    let db = db().await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);

    let signed_up = sign_up(&router, "ada@example.com").await;
    assert_eq!(signed_up.headers().get(LOCATION).unwrap(), "/admin");
    let session = session_cookie_value(&signed_up).expect("the sign-up starts a session");
    let client = TestClient::new(&router).cookie(SESSION_COOKIE, &session);

    let page = client.get("/admin/tickets").await;
    assert_eq!(page.status(), 307);
    assert_eq!(page.headers().get(LOCATION).unwrap(), "/admin/verify-email");
    assert_eq!(client.get("/admin/verify-email").await.status(), 200);
    let mutation = client.submit("/admin/tickets/-/columns", "").await;
    assert_eq!(mutation.status(), 403);

    let sent = mails(&mail, 1).await;
    assert_eq!(sent[0].to()[0].address(), "ada@example.com");
    let link = link(&sent[0]);
    assert!(link.starts_with("/admin/verify-email/confirm?token="));
    let confirmed = client.get(&link).await;
    assert_eq!(confirmed.status(), 303);
    assert_eq!(confirmed.headers().get(LOCATION).unwrap(), "/admin");
    assert_eq!(confirmed.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(flash(&confirmed).status, NotificationStatus::Success);
    assert!(
        admin(&db, "ada@example.com")
            .await
            .email_verified_at
            .is_some()
    );
    assert_eq!(client.get("/admin/tickets").await.status(), 200);
    let notice = client.get("/admin/verify-email").await;
    assert_eq!(notice.headers().get(LOCATION).unwrap(), "/admin");

    assert_eq!(client.get(&link).await.status(), 404);
}

/// An account asks for a link from the notice; the link verifies in a browser signed in as nobody,
/// which then lands on the login page.
#[tokio::test]
async fn a_requested_link_verifies_in_a_browser_that_is_not_signed_in() {
    let db = db().await;
    let ada = unverified(&db, "ada@example.com").await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail);
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;

    let requested = client.submit("/admin/verify-email", "").await;
    assert_eq!(requested.status(), 303);
    assert_eq!(
        requested.headers().get(LOCATION).unwrap(),
        "/admin/verify-email"
    );
    let link = link(&mails(&mail, 1).await[0]);

    let confirmed = TestClient::new(&router).get(&link).await;
    assert_eq!(confirmed.headers().get(LOCATION).unwrap(), "/admin/login");
    assert!(
        admin(&db, "ada@example.com")
            .await
            .email_verified_at
            .is_some()
    );
    assert_eq!(client.get("/admin/tickets").await.status(), 200);
}

/// A link verifies nothing once it expires, once the account stores another address, or on
/// another panel.
#[tokio::test]
async fn a_link_verifies_only_the_address_it_was_mailed_to_while_it_lasts() {
    let db = db().await;
    let ada = unverified(&db, "ada@example.com").await;
    let mail = MemoryTransport::new();
    let router = Router::builder()
        .discover()
        .with(db.clone())
        .mail(MailConfig::builder().transport(mail.clone()).build())
        .panel(
            Panel::new("admin")
                .auth(verifying(EmailVerification::new(PasswordAuth)))
                .resource::<TicketResource>(),
        )
        .unwrap()
        .panel(
            Panel::new("staff")
                .auth(verifying(EmailVerification::new(PasswordAuth)))
                .resource::<TicketResource>(),
        )
        .unwrap()
        .build();
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    client.submit("/admin/verify-email", "").await;
    let link = link(&mails(&mail, 1).await[0]);

    let elsewhere = link.replacen("/admin/", "/staff/", 1);
    assert_eq!(client.get(&elsewhere).await.status(), 404);

    AdminUser::filter(AdminUser::fields().id().eq(ada.id))
        .update()
        .email("ada@elsewhere.example".to_string())
        .exec(&mut db.clone())
        .await
        .unwrap();
    assert_eq!(client.get(&link).await.status(), 404);
    AdminUser::filter(AdminUser::fields().id().eq(ada.id))
        .update()
        .email("ada@example.com".to_string())
        .exec(&mut db.clone())
        .await
        .unwrap();

    AuthToken::all()
        .update()
        .expires_at(Timestamp::now() - Duration::from_secs(1))
        .exec(&mut db.clone())
        .await
        .unwrap();
    assert_eq!(client.get(&link).await.status(), 404);
    assert_eq!(
        client
            .get("/admin/verify-email/confirm?token=nope")
            .await
            .status(),
        404
    );
    assert!(
        admin(&db, "ada@example.com")
            .await
            .email_verified_at
            .is_none()
    );
}

/// The notice mails no more links than its throttle allows one account.
#[tokio::test]
async fn link_requests_past_the_throttle_answer_429() {
    let db = db().await;
    let ada = unverified(&db, "ada@example.com").await;
    let mail = MemoryTransport::new();
    let verification = EmailVerification::new(PasswordAuth)
        .throttle(LoginThrottle::new(1, Duration::from_mins(1)));
    let router = mount(db.clone(), verifying(verification), &mail).expect("panel builds");
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;

    assert_eq!(client.submit("/admin/verify-email", "").await.status(), 303);
    assert_eq!(client.submit("/admin/verify-email", "").await.status(), 429);
    assert_eq!(mails(&mail, 1).await.len(), 1);
}

fn kinds(mounted: topcoat::Result<Router>) -> Vec<DeclarationErrorKind> {
    refusal(mounted)
        .into_iter()
        .map(|error| error.kind)
        .collect()
}

/// Mounting refuses a verification it cannot serve.
#[tokio::test]
async fn mounting_refuses_a_verification_the_panel_cannot_serve() {
    let mail = MemoryTransport::new();
    let verification = || EmailVerification::new(PasswordAuth);

    assert_eq!(
        kinds(mount(
            db().await,
            Auth::disabled().email_verification(verification()),
            &mail
        )),
        vec![DeclarationErrorKind::EmailVerification(
            VerificationFault::AuthDisabled
        )]
    );
    assert_eq!(
        kinds(mount(
            db().await,
            Auth::password().email_verification(verification()),
            &mail
        )),
        vec![DeclarationErrorKind::AuthMail(AuthMailFault::Unset {
            feature: "email_verification"
        })]
    );
    let without_tokens = test_db(toasty::models!(Ticket, AdminUser, AuthSession)).await;
    assert_eq!(
        kinds(mount(
            without_tokens,
            mailing().email_verification(verification()),
            &mail
        )),
        vec![DeclarationErrorKind::MissingAuthModels {
            models: vec!["AuthToken"]
        }]
    );
}
