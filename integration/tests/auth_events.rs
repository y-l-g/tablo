//! Auth events: what a panel's `AuthHook`s receive, as the audit trail stores it, and the events a
//! failing hook stops.

use tablo::{
    Auth, AuthEvent, AuthEventKind, AuthHook, Panel, PasswordAuth, RouterBuilderPanelExt,
    TwoFactor,
    audit::{AuditEntry, PanelAuditExt},
    auth::{AdminUser, AuthFactor, AuthSession, AuthToken, create_admin},
    testing::{
        TestClient, authenticator_code_at, body_string, form_body, now, recovery_codes,
        response_cookies, two_factor_secret,
    },
};
use toasty::Db;
use topcoat::{
    context::Cx,
    cookie::Key,
    router::{Router, RouterBuilderDiscoverExt},
};

use crate::common::test_db;

const PASSWORD: &str = "correct horse battery staple";

/// A database holding Ada, whose password is [`PASSWORD`].
async fn db() -> (Db, AdminUser) {
    let db = test_db(toasty::models!(
        AdminUser,
        AuthSession,
        AuthToken,
        AuthFactor,
        AuditEntry
    ))
    .await;
    let ada = create_admin(&mut db.clone(), "ada@example.com", PASSWORD, "Ada")
        .await
        .unwrap();
    (db, ada)
}

/// The `admin` panel signing admins in by password, with a profile page and an optional second
/// factor, as `customize` leaves it.
fn router(db: Db, customize: impl FnOnce(Panel) -> Panel) -> Router {
    let auth = Auth::password()
        .profile(PasswordAuth::new())
        .two_factor(TwoFactor::new());
    let mut builder = Router::builder().discover().with(db).with(Key::generate());
    builder
        .panel(customize(Panel::new("admin").auth(auth)))
        .unwrap();
    builder.build()
}

/// The trail's entries, oldest first.
async fn entries(db: &Db) -> Vec<AuditEntry> {
    let mut entries = AuditEntry::all().exec(&mut db.clone()).await.unwrap();
    entries.sort_by_key(|entry| entry.created_at);
    entries
}

/// The events the trail holds, oldest first.
async fn events(db: &Db) -> Vec<String> {
    entries(db)
        .await
        .into_iter()
        .map(|entry| entry.event)
        .collect()
}

/// A refused sign-in, a sign-in and a sign-out each leave an entry: the refused one names the
/// login typed and no user, the others name the account.
#[tokio::test]
async fn signing_in_and_out_is_recorded() {
    let (db, ada) = db().await;
    let router = router(db.clone(), PanelAuditExt::audit_trail);

    let (_, refused) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", "wrong", "")
        .await;
    assert_eq!(refused.status(), 403);
    let (client, admitted) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", PASSWORD, "")
        .await;
    assert_eq!(admitted.status(), 303);
    assert_eq!(client.submit("/admin/logout", "").await.status(), 303);

    let entries = entries(&db).await;
    let names: Vec<&str> = entries.iter().map(|entry| entry.event.as_str()).collect();
    assert_eq!(names, ["sign_in_failed", "sign_in", "sign_out"]);
    assert_eq!(entries[0].actor_id, None);
    assert_eq!(entries[0].record, None);
    let typed = &entries[0].changes[0];
    assert_eq!(
        (typed.field.as_str(), typed.after.as_deref()),
        ("login", Some("ada@example.com"))
    );
    let id = ada.id.to_string();
    for entry in &entries[1..] {
        assert_eq!(entry.actor_id.as_ref(), Some(&id), "{entry:?}");
        assert_eq!(entry.record.as_ref(), Some(&id), "{entry:?}");
    }
}

/// Turning a second factor on, a wrong code, the sign-in a right code completes, and turning the
/// factor off are each recorded; the password alone records no sign-in.
#[tokio::test]
async fn a_second_factor_is_recorded() {
    let (db, ada) = db().await;
    let router = router(db.clone(), PanelAuditExt::audit_trail);
    let code = |secret: &str, steps: i64| {
        authenticator_code_at(secret, now().saturating_add_signed(steps * 30))
    };

    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let started = client
        .submit(
            "/admin/two-factor",
            &form_body(&[("intent", "start"), ("current_password", PASSWORD)]),
        )
        .await;
    let secret = two_factor_secret(&body_string(started).await).expect("the set-up shows its key");
    let confirmed = client
        .submit(
            "/admin/two-factor",
            &form_body(&[("intent", "confirm"), ("code", &code(&secret, 0))]),
        )
        .await;
    assert_eq!(confirmed.status(), 200);
    let recovery = recovery_codes(&body_string(confirmed).await);

    let (challenged, response) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", PASSWORD, "")
        .await;
    assert_eq!(response.status(), 303);
    let answer = |code: String| {
        let challenged = &challenged;
        async move {
            challenged
                .submit(
                    "/admin/two-factor-challenge",
                    &form_body(&[("code", &code)]),
                )
                .await
        }
    };
    assert_eq!(answer("000000".to_string()).await.status(), 422);
    assert_eq!(
        events(&db).await,
        ["two_factor_enabled", "second_factor_failed"],
        "the password alone signs nobody in"
    );
    let admitted = answer(code(&secret, 1)).await;
    assert_eq!(admitted.status(), 303);

    let signed = challenged.cookies(&response_cookies(&admitted));
    let off = signed
        .submit(
            "/admin/two-factor",
            &form_body(&[("intent", "disable"), ("code", &recovery[0])]),
        )
        .await;
    assert_eq!(off.status(), 303);
    assert_eq!(
        events(&db).await,
        [
            "two_factor_enabled",
            "second_factor_failed",
            "sign_in",
            "two_factor_disabled"
        ]
    );
}

/// A password changed on the profile page is recorded with the change.
#[tokio::test]
async fn a_changed_password_is_recorded() {
    let (db, ada) = db().await;
    let router = router(db.clone(), PanelAuditExt::audit_trail);
    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    let saved = client
        .submit(
            "/admin/profile",
            &form_body(&[
                ("name", "Ada"),
                ("email", "ada@example.com"),
                ("current_password", PASSWORD),
                ("password", "a brand new passphrase"),
                ("password_confirmation", "a brand new passphrase"),
            ]),
        )
        .await;
    assert_eq!(saved.status(), 303);
    assert_eq!(events(&db).await, ["credentials_changed"]);
}

/// Fails on every event, as an audit store that is down would.
struct Down;

impl AuthHook for Down {
    async fn on_event(
        _cx: &Cx,
        _event: &AuthEvent<'_>,
        _ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        Err(topcoat::router::error::internal_server_error(std::io::Error::other("down")).into())
    }
}

/// A sign-in its hook cannot record opens no session; a refused sign-in and a sign-out go through
/// unrecorded.
#[tokio::test]
async fn a_failing_hook_stops_a_sign_in_and_no_sign_out() {
    let (db, ada) = db().await;
    let router = router(db.clone(), |panel| panel.auth_hook::<Down>());
    let sessions = || async {
        AuthSession::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .len()
    };

    let (_, refused) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", "wrong", "")
        .await;
    assert_eq!(refused.status(), 403);
    let (_, response) = TestClient::new(&router)
        .log_in("admin", "ada@example.com", PASSWORD, "")
        .await;
    assert_ne!(response.status(), 303);
    assert_eq!(sessions().await, 0, "the sign-in opened no session");

    let client = TestClient::new(&router).sign_in(&db, "admin", &ada).await;
    assert_eq!(sessions().await, 1);
    assert_eq!(client.submit("/admin/logout", "").await.status(), 303);
    assert_eq!(sessions().await, 0, "the sign-out went through");
}

/// A hook reads what happened and to whom from the event.
#[tokio::test]
async fn a_hook_reads_the_event() {
    struct Reads;

    impl AuthHook for Reads {
        async fn on_event(
            _cx: &Cx,
            event: &AuthEvent<'_>,
            _ex: &mut dyn toasty::Executor,
        ) -> topcoat::Result<()> {
            if let AuthEventKind::SignInFailed { login } = event.kind() {
                assert_eq!(login, "nobody@example.com");
                assert!(event.actor().is_none());
                assert_eq!(event.target_id(), None);
                assert_eq!(event.panel().to_string(), "/admin");
            }
            Ok(())
        }
    }

    let (db, _) = db().await;
    let router = router(db, |panel| panel.auth_hook::<Reads>());
    let (_, refused) = TestClient::new(&router)
        .log_in("admin", "nobody@example.com", "wrong", "")
        .await;
    assert_eq!(refused.status(), 403);
}
