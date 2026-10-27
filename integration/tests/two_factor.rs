//! Two-factor sign-in: setting up an authenticator, the challenge a password then leads to, the
//! codes it takes once, recovery codes, turning it off, requiring it, and the mistakes mounting
//! refuses.

use http::header::LOCATION;
use tablo::{
    Auth, DeclarationErrorKind, LoginThrottle, NoForm, Panel, ReadOnly, Resource, ResourceDef,
    RouterBuilderPanelExt, Table, TextColumn, TwoFactor, TwoFactorFault,
    auth::{AdminUser, AuthFactor, AuthSession, AuthToken, FactorKind, create_admin},
    lens,
    testing::{
        SESSION_COOKIE, TestClient, authenticator_code_at, body_string, form_body, now,
        recovery_codes, refusal, session_cookie_value, two_factor_secret, two_factor_state,
    },
};
use toasty::Db;
use topcoat::{
    cookie::Key,
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use crate::common::test_db;

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Invoice {
    #[key]
    #[auto]
    id: Uuid,
    number: String,
}

struct InvoiceResource;

impl Resource for InvoiceResource {
    type Model = Invoice;
    type Form = NoForm<Invoice>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Invoice.number))))
    }
}

const PASSWORD: &str = "a password";
const CHALLENGE: &str = "/admin/two-factor-challenge";
const SETTINGS: &str = "/admin/two-factor";

/// A database holding Ada, whose password is [`PASSWORD`].
async fn db() -> (Db, AdminUser) {
    let db = test_db(toasty::models!(
        Invoice,
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

/// The `admin` panel signing in by password with `two_factor`, under a router holding a cookie
/// key.
fn mount(db: Db, auth: Auth) -> topcoat::Result<Router> {
    Ok(Router::builder()
        .discover()
        .with(db)
        .with(Key::generate())
        .panel(Panel::new("admin").auth(auth).resource::<InvoiceResource>())?
        .build())
}

fn router(db: Db, two_factor: TwoFactor) -> Router {
    mount(db, Auth::password().two_factor(two_factor)).expect("panel builds")
}

/// The code an authenticator holding `secret` shows `steps` 30-second steps from now. The panel
/// takes the step either side of its own, and each step once.
fn code(secret: &str, steps: i64) -> String {
    authenticator_code_at(secret, now().saturating_add_signed(steps * 30))
}

/// A six-digit code no step around now shows for `secret`: a code the panel refuses.
fn wrong(secret: &str) -> String {
    let near: Vec<String> = (-1..=1).map(|steps| code(secret, steps)).collect();
    ["000000", "111111", "222222", "333333"]
        .into_iter()
        .find(|candidate| !near.iter().any(|code| code == candidate))
        .expect("three codes cannot cover four candidates")
        .to_string()
}

/// Ada, signed in, turns two-factor sign-in on with the current code after her password;
/// answers the secret, the code that confirmed it, her recovery codes and the client holding her
/// new session.
async fn enrol<'r>(
    router: &'r Router,
    db: &Db,
    ada: &AdminUser,
) -> (String, String, Vec<String>, TestClient<'r>) {
    let client = TestClient::new(router).sign_in(db, "admin", ada).await;
    let page = body_string(client.get(SETTINGS).await).await;
    assert_eq!(two_factor_state(&page).as_deref(), Some("off"));

    let started = client
        .submit(
            SETTINGS,
            &form_body(&[("intent", "start"), ("current_password", PASSWORD)]),
        )
        .await;
    assert_eq!(started.status(), 200);
    let page = body_string(started).await;
    assert_eq!(two_factor_state(&page).as_deref(), Some("setup"));
    let secret = two_factor_secret(&page).expect("the set-up shows its key");

    let setup_code = code(&secret, 0);
    let confirmed = client
        .submit(
            SETTINGS,
            &form_body(&[("intent", "confirm"), ("code", &setup_code)]),
        )
        .await;
    assert_eq!(confirmed.status(), 200);
    let session = session_cookie_value(&confirmed).expect("turning it on renews the session");
    let codes = recovery_codes(&body_string(confirmed).await);
    assert_eq!(codes.len(), 8);
    (
        secret,
        setup_code,
        codes,
        TestClient::new(router).cookie(SESSION_COOKIE, &session),
    )
}

/// Ada's password, posted through the login form: answers the client holding the challenge.
async fn password<'r>(router: &'r Router) -> TestClient<'r> {
    let (client, response) = TestClient::new(router)
        .log_in("admin", "ada@example.com", PASSWORD, "/admin/invoices")
        .await;
    assert_eq!(response.status(), 303);
    let location = response.headers().get(LOCATION).unwrap().to_str().unwrap();
    assert!(location.starts_with(CHALLENGE), "{location}");
    assert!(
        session_cookie_value(&response).is_none(),
        "a password alone opens no session"
    );
    client
}

/// The challenge answered with `code`.
async fn answer(client: &TestClient<'_>, code: &str) -> http::Response<topcoat::router::Body> {
    client
        .submit(
            CHALLENGE,
            &form_body(&[("code", code), ("next", "/admin/invoices")]),
        )
        .await
}

async fn sessions(db: &Db) -> Vec<AuthSession> {
    AuthSession::all().exec(&mut db.clone()).await.unwrap()
}

/// Once Ada turns it on, her password leads to the challenge, and only a code from her
/// authenticator signs her in, landing where she was going.
#[tokio::test]
async fn an_enrolled_account_signs_in_with_its_code_after_its_password() {
    let (db, ada) = db().await;
    let router = router(db.clone(), TwoFactor::new());
    let (secret, _, _, signed) = enrol(&router, &db, &ada).await;
    let page = body_string(signed.get(SETTINGS).await).await;
    assert_eq!(two_factor_state(&page).as_deref(), Some("on"));

    let challenged = password(&router).await;
    assert_eq!(challenged.get(CHALLENGE).await.status(), 200);
    assert_eq!(
        challenged.get("/admin/invoices").await.status(),
        307,
        "the challenge opens nothing else"
    );
    assert_eq!(answer(&challenged, &wrong(&secret)).await.status(), 422);

    let signed_in = answer(&challenged, &code(&secret, 1)).await;
    assert_eq!(signed_in.status(), 303);
    assert_eq!(
        signed_in.headers().get(LOCATION).unwrap(),
        "/admin/invoices"
    );
    let session = session_cookie_value(&signed_in).expect("the code opens a session");
    let client = TestClient::new(&router).cookie(SESSION_COOKIE, &session);
    assert_eq!(client.get("/admin/invoices").await.status(), 200);
    assert_eq!(
        challenged.get(CHALLENGE).await.status(),
        303,
        "a challenge answers once"
    );
}

/// An account without an authenticator signs in with its password alone.
#[tokio::test]
async fn an_account_without_an_authenticator_signs_in_with_its_password() {
    let (db, _) = db().await;
    let router = router(db, TwoFactor::new());
    let (_, response) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", PASSWORD, "")
        .await;
    assert_eq!(response.headers().get(LOCATION).unwrap(), "/admin");
    assert!(session_cookie_value(&response).is_some());
}

/// A code that signed in once, or confirmed the set-up, does not sign in again.
#[tokio::test]
async fn an_authenticator_code_signs_in_once() {
    let (db, ada) = db().await;
    let router = router(db.clone(), TwoFactor::new());
    let (secret, setup_code, _, _) = enrol(&router, &db, &ada).await;

    let challenged = password(&router).await;
    assert_eq!(
        answer(&challenged, &setup_code).await.status(),
        422,
        "the set-up's code is spent"
    );
    assert_eq!(answer(&challenged, &code(&secret, 1)).await.status(), 303);
    let again = password(&router).await;
    assert_eq!(answer(&again, &code(&secret, 1)).await.status(), 422);
}

/// A code typed as an app shows it, in two groups of three, signs in.
#[tokio::test]
async fn a_spaced_code_signs_in() {
    let (db, ada) = db().await;
    let router = router(db.clone(), TwoFactor::new());
    let (secret, _, _, _) = enrol(&router, &db, &ada).await;

    let challenged = password(&router).await;
    let next = code(&secret, 1);
    let spaced = format!("{} {}", &next[..3], &next[3..]);
    assert_eq!(answer(&challenged, &spaced).await.status(), 303);
}

/// A challenge answered without the form's CSRF token is refused, and leaves the challenge
/// standing.
#[tokio::test]
async fn a_forged_challenge_answer_is_refused() {
    let (db, ada) = db().await;
    let router = router(db.clone(), TwoFactor::new());
    let (secret, _, _, _) = enrol(&router, &db, &ada).await;

    let challenged = password(&router).await;
    let forged = challenged
        .post_form(CHALLENGE, form_body(&[("code", &code(&secret, 1))]))
        .await;
    assert_eq!(forged.status(), 403);
    assert_eq!(answer(&challenged, &code(&secret, 1)).await.status(), 303);
}

/// Setting up an authenticator takes the current password: a session alone sets none up.
#[tokio::test]
async fn a_set_up_takes_the_current_password() {
    let (db, ada) = db().await;
    let router = router(db.clone(), TwoFactor::new());
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    for password in ["", "not her password"] {
        let refused = client
            .submit(
                SETTINGS,
                &form_body(&[("intent", "start"), ("current_password", password)]),
            )
            .await;
        let page = body_string(refused).await;
        assert_eq!(two_factor_state(&page).as_deref(), Some("off"));
        assert!(two_factor_secret(&page).is_none());
    }
    assert!(
        AuthFactor::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_empty()
    );
}

/// The set-up and the recovery codes reach no cache, and the stored secret is not the key the
/// app reads.
#[tokio::test]
async fn the_secret_and_the_codes_stay_out_of_caches_and_the_database() {
    let (db, ada) = db().await;
    let router = router(db.clone(), TwoFactor::new());
    let (secret, _, codes, client) = enrol(&router, &db, &ada).await;
    let page = client.get(SETTINGS).await;
    assert_eq!(
        page.headers().get(http::header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let stored = AuthFactor::all().exec(&mut db.clone()).await.unwrap();
    for secret in codes.iter().chain([&secret]) {
        assert!(
            stored
                .iter()
                .all(|factor| !factor.secret.contains(secret.as_str()))
        );
    }
}

/// A recovery code signs in in place of the app, once.
#[tokio::test]
async fn a_recovery_code_signs_in_once() {
    let (db, ada) = db().await;
    let router = router(db.clone(), TwoFactor::new());
    let (_, _, codes, _) = enrol(&router, &db, &ada).await;

    let challenged = password(&router).await;
    assert_eq!(
        answer(&challenged, &codes[0].to_uppercase()).await.status(),
        303
    );
    let again = password(&router).await;
    assert_eq!(answer(&again, &codes[0]).await.status(), 422);
    assert_eq!(answer(&again, &codes[1]).await.status(), 303);
}

/// Turning it off takes a code, and ends Ada's other sessions; her password alone then signs in.
#[tokio::test]
async fn turning_it_off_takes_a_code_and_ends_the_other_sessions() {
    let (db, ada) = db().await;
    let router = router(db.clone(), TwoFactor::new());
    let (secret, _, _, client) = enrol(&router, &db, &ada).await;
    tablo::auth::mint_session(&mut db.clone(), "admin", &ada)
        .await
        .unwrap();

    let refused = client
        .submit(
            SETTINGS,
            &form_body(&[("intent", "disable"), ("code", &wrong(&secret))]),
        )
        .await;
    assert_eq!(refused.status(), 200);
    assert_eq!(
        two_factor_state(&body_string(refused).await).as_deref(),
        Some("on")
    );
    assert_eq!(sessions(&db).await.len(), 2);

    let disabled = client
        .submit(
            SETTINGS,
            &form_body(&[("intent", "disable"), ("code", &code(&secret, 1))]),
        )
        .await;
    assert_eq!(disabled.headers().get(LOCATION).unwrap(), SETTINGS);
    assert_eq!(
        sessions(&db).await.len(),
        1,
        "only this browser stays signed in"
    );
    assert!(
        AuthFactor::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .is_empty()
    );

    let (_, response) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", PASSWORD, "")
        .await;
    assert!(session_cookie_value(&response).is_some());
}

/// Fresh recovery codes take a code, and replace the earlier ones.
#[tokio::test]
async fn new_recovery_codes_replace_the_earlier_ones() {
    let (db, ada) = db().await;
    let router = router(db.clone(), TwoFactor::new());
    let (_, _, first, client) = enrol(&router, &db, &ada).await;

    let issued = client
        .submit(
            SETTINGS,
            &form_body(&[("intent", "recovery-codes"), ("code", &first[0])]),
        )
        .await;
    let fresh = recovery_codes(&body_string(issued).await);
    assert_eq!(fresh.len(), 8);

    let challenged = password(&router).await;
    assert_eq!(answer(&challenged, &first[1]).await.status(), 422);
    assert_eq!(answer(&challenged, &fresh[0]).await.status(), 303);
}

/// Accounts renewing their recovery codes at once each get theirs: a transaction the database
/// aborts for another account's runs again rather than answering an outage.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn recovery_codes_renewed_by_several_accounts_at_once_each_arrive() {
    const ACCOUNTS: usize = 8;
    let (db, _) = db().await;
    // Leaked so that each renewal's client outlives the test's stack in its own task.
    let router: &'static Router = Box::leak(Box::new(router(db.clone(), TwoFactor::new())));
    let mut enrolled = Vec::new();
    for n in 0..ACCOUNTS {
        let email = format!("admin{n}@example.com");
        let admin = create_admin(&mut db.clone(), &email, PASSWORD, "Admin")
            .await
            .unwrap();
        let (_, _, codes, client) = enrol(router, &db, &admin).await;
        enrolled.push((codes.into_iter().next().unwrap(), client));
    }

    let renewals: Vec<_> = enrolled
        .into_iter()
        .map(|(code, client)| {
            tokio::spawn(async move {
                let issued = client
                    .submit(
                        SETTINGS,
                        &form_body(&[("intent", "recovery-codes"), ("code", &code)]),
                    )
                    .await;
                assert_eq!(issued.status(), 200);
                recovery_codes(&body_string(issued).await)
            })
        })
        .collect();
    for renewal in renewals {
        assert_eq!(renewal.await.unwrap().len(), 8);
    }
    let stored = AuthFactor::all().exec(&mut db.clone()).await.unwrap();
    assert_eq!(
        stored
            .iter()
            .filter(|factor| factor.kind == FactorKind::RecoveryCode)
            .count(),
        ACCOUNTS * 8
    );
}

/// A panel that requires it sends a signed-in account without an authenticator to set one up
/// first: pages redirect there, and other requests answer 403.
#[tokio::test]
async fn a_required_authenticator_comes_before_the_panel() {
    let (db, ada) = db().await;
    let router = router(db.clone(), TwoFactor::new().required());
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let diverted = client.get("/admin/invoices").await;
    assert_eq!(diverted.headers().get(LOCATION).unwrap(), SETTINGS);
    assert_eq!(client.get(SETTINGS).await.status(), 200);

    let (_, _, _, client) = enrol(&router, &db, &ada).await;
    assert_eq!(client.get("/admin/invoices").await.status(), 200);
}

/// Past its throttle the challenge answers 429, even to the right code.
#[tokio::test]
async fn the_challenge_throttles_its_codes() {
    let (db, ada) = db().await;
    let throttle = LoginThrottle::new(2, std::time::Duration::from_mins(1));
    let router = router(db.clone(), TwoFactor::new().throttle(throttle));
    let (secret, _, _, _) = enrol(&router, &db, &ada).await;

    let challenged = password(&router).await;
    for _ in 0..2 {
        assert_eq!(answer(&challenged, &wrong(&secret)).await.status(), 422);
    }
    assert_eq!(answer(&challenged, &code(&secret, 1)).await.status(), 429);
}

/// Without a live challenge, its page sends the browser back to the login.
#[tokio::test]
async fn the_challenge_page_without_a_challenge_leads_to_the_login() {
    let (db, _) = db().await;
    let router = router(db, TwoFactor::new());
    let page = TestClient::new(&router).get(CHALLENGE).await;
    assert_eq!(page.status(), 303);
    assert_eq!(page.headers().get(LOCATION).unwrap(), "/admin/login");
    let posted = answer(&TestClient::new(&router), "123456").await;
    assert_eq!(posted.status(), 303);
}

fn kinds(mounted: topcoat::Result<Router>) -> Vec<DeclarationErrorKind> {
    refusal(mounted)
        .into_iter()
        .map(|error| error.kind)
        .collect()
}

/// Mounting refuses two-factor sign-in it cannot serve: without auth, without a cookie key to
/// seal the secrets, or without the model that holds them.
#[tokio::test]
async fn mounting_refuses_two_factor_sign_in_the_panel_cannot_serve() {
    let (db, _) = db().await;
    assert_eq!(
        kinds(mount(
            db.clone(),
            Auth::disabled().two_factor(TwoFactor::new())
        )),
        vec![DeclarationErrorKind::TwoFactor(
            TwoFactorFault::AuthDisabled
        )]
    );
    let keyless = Router::builder()
        .discover()
        .with(db)
        .panel(Panel::new("admin").auth(Auth::password().two_factor(TwoFactor::new())))
        .map(|builder| builder.build());
    assert_eq!(
        kinds(keyless),
        vec![DeclarationErrorKind::TwoFactor(TwoFactorFault::MissingKey)]
    );
    let unregistered = test_db(toasty::models!(Invoice, AdminUser, AuthSession, AuthToken)).await;
    assert_eq!(
        kinds(mount(
            unregistered,
            Auth::password().two_factor(TwoFactor::new())
        )),
        vec![DeclarationErrorKind::MissingAuthModels {
            models: vec!["AuthFactor"],
        }]
    );
}

/// Every fifth wrong code in a row ends the challenge, so the next round needs the password; past
/// the fifth each code waits, longer each time, while a recovery code still signs in and clears
/// the count.
#[tokio::test]
async fn wrong_codes_end_the_challenge_and_make_the_next_ones_wait() {
    let (db, ada) = db().await;
    let throttle = LoginThrottle::new(100, std::time::Duration::from_mins(1));
    let router = router(db.clone(), TwoFactor::new().throttle(throttle));
    let (secret, _, codes, _) = enrol(&router, &db, &ada).await;

    let challenged = password(&router).await;
    for _ in 0..4 {
        assert_eq!(answer(&challenged, &wrong(&secret)).await.status(), 422);
    }
    let ended = answer(&challenged, &wrong(&secret)).await;
    assert_eq!(ended.status(), 303);
    let location = ended.headers().get(LOCATION).unwrap().to_str().unwrap();
    assert!(location.starts_with("/admin/login"), "{location}");
    assert!(
        answer(&challenged, &code(&secret, 0))
            .await
            .headers()
            .get(LOCATION)
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("/admin/login"),
        "the ended challenge takes no code"
    );

    let again = password(&router).await;
    assert_eq!(
        answer(&again, &code(&secret, 0)).await.status(),
        429,
        "even the right code waits"
    );
    assert_eq!(answer(&again, &codes[0]).await.status(), 303);
    assert!(
        AuthFactor::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .iter()
            .all(|factor| factor.failures == 0 && factor.retry_at.is_none()),
        "signing in clears the count"
    );
}
