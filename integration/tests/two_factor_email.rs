//! Codes mailed as a second factor: setting them up, the code each sign-in mails, a new code in
//! place of the last, the throttle on mailing them, and the mistakes mounting refuses.

use http::header::LOCATION;
use tablo::{
    Auth, AuthMail, AuthMailFault, DeclarationErrorKind, LoginThrottle, Panel,
    RouterBuilderPanelExt, TwoFactor,
    auth::{AdminUser, AuthFactor, AuthSession, AuthToken, create_admin},
    testing::{
        SESSION_COOKIE, TestClient, body_string, form_body, recovery_codes, refusal,
        session_cookie_value, two_factor_state,
    },
};
use toasty::Db;
use topcoat::{
    cookie::Key,
    mail::{MailConfig, Mailbox, MemoryTransport, RouterBuilderMailExt},
    router::{Router, RouterBuilderDiscoverExt},
};

use crate::common::{
    mail::{BASE_URL, code},
    test_db,
};

const PASSWORD: &str = "a password";
const CHALLENGE: &str = "/admin/two-factor-challenge";
const SETTINGS: &str = "/admin/two-factor";

/// A database holding Ada, whose password is [`PASSWORD`].
async fn db() -> (Db, AdminUser) {
    let db = test_db(toasty::models!(
        AdminUser,
        AuthSession,
        AuthToken,
        AuthFactor
    ))
    .await;
    let ada = create_admin(&mut db.clone(), "ada@example.com", PASSWORD, "Ada")
        .await
        .unwrap();
    (db, ada)
}

/// The `admin` panel signing in by password with `two_factor`, mailing through `mail`.
fn router(db: Db, mail: &MemoryTransport, two_factor: TwoFactor) -> Router {
    keyed_router(db, mail, two_factor, Key::generate())
}

/// [`router`] under the cookie key `key`, which seals what a factor stores.
fn keyed_router(db: Db, mail: &MemoryTransport, two_factor: TwoFactor, key: Key) -> Router {
    let auth = Auth::password()
        .mail(AuthMail::new(
            Mailbox::new("no-reply@example.com").unwrap(),
            BASE_URL,
        ))
        .two_factor(two_factor);
    Router::builder()
        .discover()
        .with(db)
        .with(key)
        .mail(MailConfig::builder().transport(mail.clone()).build())
        .panel(Panel::new("admin").auth(auth))
        .expect("panel builds")
        .build()
}

/// The code in the last mail `mail` sent, to Ada.
fn last_code(mail: &MemoryTransport) -> String {
    let sent = mail.sent();
    let last = sent.last().expect("a mail was sent");
    assert_eq!(last.to()[0].address(), "ada@example.com");
    code(last)
}

/// Ada, signed in, turns codes by email on with the code mailed to her; answers the client
/// holding her new session.
async fn enrol<'r>(
    router: &'r Router,
    db: &Db,
    ada: &AdminUser,
    mail: &MemoryTransport,
) -> TestClient<'r> {
    enrol_with_codes(router, db, ada, mail).await.0
}

/// [`enrol`], answering her recovery codes too.
async fn enrol_with_codes<'r>(
    router: &'r Router,
    db: &Db,
    ada: &AdminUser,
    mail: &MemoryTransport,
) -> (TestClient<'r>, Vec<String>) {
    let client = TestClient::new(router).sign_in(db, "admin", ada).await;
    let started = client
        .submit(
            SETTINGS,
            &form_body(&[("intent", "start-email"), ("current_password", PASSWORD)]),
        )
        .await;
    assert_eq!(started.headers().get(LOCATION).unwrap(), SETTINGS);
    let page = body_string(client.get(SETTINGS).await).await;
    assert_eq!(two_factor_state(&page).as_deref(), Some("setup"));

    let confirmed = client
        .submit(
            SETTINGS,
            &form_body(&[("intent", "confirm"), ("code", &last_code(mail))]),
        )
        .await;
    assert_eq!(confirmed.status(), 200);
    let session = session_cookie_value(&confirmed).expect("turning it on renews the session");
    let codes = recovery_codes(&body_string(confirmed).await);
    assert_eq!(codes.len(), 8);
    (
        TestClient::new(router).cookie(SESSION_COOKIE, &session),
        codes,
    )
}

/// Ada's password, posted through the login form: answers the client holding the challenge.
async fn password<'r>(router: &'r Router) -> TestClient<'r> {
    let (client, response) = TestClient::new(router)
        .log_in("admin", "ada@example.com", PASSWORD, "")
        .await;
    let location = response.headers().get(LOCATION).unwrap().to_str().unwrap();
    assert!(location.starts_with(CHALLENGE), "{location}");
    assert!(session_cookie_value(&response).is_none());
    client
}

/// The challenge answered with `fields`.
async fn post(client: &TestClient<'_>, fields: &[(&str, &str)]) -> http::StatusCode {
    client.submit(CHALLENGE, &form_body(fields)).await.status()
}

/// Each sign-in mails Ada a code, which signs her in once; a code mailed earlier does not.
#[tokio::test]
async fn each_sign_in_mails_a_code_that_works_once() {
    let (db, ada) = db().await;
    let mail = MemoryTransport::new();
    let generous = LoginThrottle::new(10, std::time::Duration::from_mins(15));
    let router = router(
        db.clone(),
        &mail,
        TwoFactor::new().email_codes().mail_throttle(generous),
    );
    let signed = enrol(&router, &db, &ada, &mail).await;
    let page = body_string(signed.get(SETTINGS).await).await;
    assert_eq!(two_factor_state(&page).as_deref(), Some("on"));
    let mailed = mail.sent().len();

    let challenged = password(&router).await;
    assert_eq!(mail.sent().len(), mailed + 1, "the password mails a code");
    let first = last_code(&mail);
    assert_eq!(challenged.get(CHALLENGE).await.status(), 200);
    assert_eq!(post(&challenged, &[("code", &first)]).await, 303);

    let again = password(&router).await;
    assert_eq!(post(&again, &[("code", &first)]).await, 422);
    assert_eq!(post(&again, &[("code", &last_code(&mail))]).await, 303);
}

/// A new code replaces the last one, and past the throttle the challenge mails no more.
#[tokio::test]
async fn a_new_code_replaces_the_last_until_the_throttle_holds() {
    let (db, ada) = db().await;
    let mail = MemoryTransport::new();
    // The set-up mails one, the password a second, the request a third.
    let router = router(db.clone(), &mail, TwoFactor::new().email_codes());
    enrol(&router, &db, &ada, &mail).await;

    let challenged = password(&router).await;
    let replaced = last_code(&mail);
    assert_eq!(post(&challenged, &[("intent", "send")]).await, 200);
    let fresh = last_code(&mail);
    assert_eq!(post(&challenged, &[("intent", "send")]).await, 429);
    assert_eq!(post(&challenged, &[("code", &replaced)]).await, 422);
    assert_eq!(post(&challenged, &[("code", &fresh)]).await, 303);
}

/// Turning codes by email off takes a mailed code, and leaves the password alone to sign in.
#[tokio::test]
async fn turning_mailed_codes_off_takes_a_mailed_code() {
    let (db, ada) = db().await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail, TwoFactor::new().email_codes());
    let client = enrol(&router, &db, &ada, &mail).await;

    let sent = client
        .submit(SETTINGS, &form_body(&[("intent", "send")]))
        .await;
    assert_eq!(sent.headers().get(LOCATION).unwrap(), SETTINGS);
    let disabled = client
        .submit(
            SETTINGS,
            &form_body(&[("intent", "disable"), ("code", &last_code(&mail))]),
        )
        .await;
    assert_eq!(disabled.headers().get(LOCATION).unwrap(), SETTINGS);

    let (_, response) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", PASSWORD, "")
        .await;
    assert!(session_cookie_value(&response).is_some());
}

/// A panel that stopped mailing codes mails none at sign-in, and a recovery code still signs in.
#[tokio::test]
async fn a_panel_that_stopped_mailing_codes_takes_a_recovery_code() {
    let (db, ada) = db().await;
    let mail = MemoryTransport::new();
    let key = Key::generate();
    let mailing = keyed_router(
        db.clone(),
        &mail,
        TwoFactor::new().email_codes(),
        key.clone(),
    );
    let (_, codes) = enrol_with_codes(&mailing, &db, &ada, &mail).await;
    let mailed = mail.sent().len();

    let router = keyed_router(db.clone(), &mail, TwoFactor::new(), key);
    let challenged = password(&router).await;
    assert_eq!(mail.sent().len(), mailed);
    assert_eq!(challenged.get(CHALLENGE).await.status(), 200);
    assert_eq!(post(&challenged, &[("code", &codes[0])]).await, 303);
}

/// Turning codes by email on takes the current password.
#[tokio::test]
async fn turning_mailed_codes_on_takes_the_current_password() {
    let (db, ada) = db().await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail, TwoFactor::new().email_codes());
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let refused = client
        .submit(
            SETTINGS,
            &form_body(&[("intent", "start-email"), ("current_password", "wrong")]),
        )
        .await;
    assert_eq!(
        two_factor_state(&body_string(refused).await).as_deref(),
        Some("off")
    );
    assert!(mail.sent().is_empty());
}

/// A panel without mailed codes offers none and refuses to set them up.
#[tokio::test]
async fn a_panel_without_mailed_codes_sets_none_up() {
    let (db, ada) = db().await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail, TwoFactor::new());
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let started = client
        .submit(
            SETTINGS,
            &form_body(&[("intent", "start-email"), ("current_password", PASSWORD)]),
        )
        .await;
    assert_eq!(started.headers().get(LOCATION).unwrap(), SETTINGS);
    assert!(mail.sent().is_empty());
    assert!(
        AuthFactor::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_empty()
    );
}

/// Mounting refuses mailed codes without a sender.
#[tokio::test]
async fn mounting_refuses_mailed_codes_without_a_sender() {
    let (db, _) = db().await;
    let mounted = Router::builder()
        .discover()
        .with(db)
        .with(Key::generate())
        .mail(
            MailConfig::builder()
                .transport(MemoryTransport::new())
                .build(),
        )
        .panel(
            Panel::new("admin").auth(Auth::password().two_factor(TwoFactor::new().email_codes())),
        )
        .map(|builder| builder.build());
    let kinds: Vec<_> = refusal(mounted)
        .into_iter()
        .map(|error| error.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![DeclarationErrorKind::AuthMail(AuthMailFault::Unset {
            feature: "two_factor"
        })]
    );
}

/// The fifth wrong code in a row mails Ada that her password signed in and codes failed.
#[tokio::test]
async fn wrong_codes_after_the_password_mail_the_account() {
    let (db, ada) = db().await;
    let mail = MemoryTransport::new();
    let router = router(db.clone(), &mail, TwoFactor::new().email_codes());
    enrol(&router, &db, &ada, &mail).await;

    let (challenged, _) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", PASSWORD, "/admin")
        .await;
    let mailed = mail.sent().len();
    let wrong = if last_code(&mail) == "000000" {
        "111111"
    } else {
        "000000"
    };
    for _ in 0..4 {
        challenged
            .submit(CHALLENGE, &form_body(&[("code", wrong)]))
            .await;
    }
    assert_eq!(mail.sent().len(), mailed, "four wrong codes mail nothing");
    challenged
        .submit(CHALLENGE, &form_body(&[("code", wrong)]))
        .await;
    let sent = mail.sent();
    assert_eq!(sent.len(), mailed + 1, "the fifth mails the account");
    assert_eq!(sent.last().unwrap().to()[0].address(), "ada@example.com");
}
