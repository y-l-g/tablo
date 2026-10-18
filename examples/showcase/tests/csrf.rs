//! The CSRF check on the auth forms: a forged sign-in, sign-up, reset, profile save, link
//! request, tenant registration or sign-out answers 403 before it changes anything.

use showcase::models::{DEMO_ADMIN_EMAIL, DEMO_ADMIN_PASSWORD};
use uuid::Uuid;

use crate::common::{
    TestClient, demo_client, form_body, full_db, routers::router_for_tests as router,
    session_cookie_value,
};

/// A forged login POST is refused before any credential work and mints no
/// session.
///
/// The CSRF verify runs before the password is read, so a mismatched token and
/// a missing `csrf_token` field are both 403 rather than a credential verdict,
/// and a rejected attempt sets no session cookie. The regression it catches is
/// a login handler that authenticates first and verifies the token (if at all)
/// afterwards.
#[tokio::test]
async fn forged_login_answers_403_and_sets_no_session() {
    let db = full_db().await;
    let router = router(db.clone());
    let field = Uuid::new_v4().to_string();
    let cookie = Uuid::new_v4().to_string();

    for (submitted, label) in [
        (Some(field.as_str()), "mismatched token"),
        (None, "missing token"),
    ] {
        let mut pairs = vec![
            ("email", DEMO_ADMIN_EMAIL),
            ("password", DEMO_ADMIN_PASSWORD),
        ];
        if let Some(field) = submitted {
            pairs.push(("csrf_token", field));
        }
        let resp = TestClient::new(&router)
            .csrf(&cookie)
            .post_form("/admin/login", form_body(&pairs))
            .await;
        assert_eq!(
            resp.status(),
            403,
            "login {label}: a forged POST must 403, got {}",
            resp.status()
        );
        assert!(
            session_cookie_value(&resp).is_none(),
            "login {label}: a forged login must set no session cookie"
        );
    }
}

/// A forged sign-up POST answers 403 before the registrar runs: no account, no session.
#[tokio::test]
async fn forged_sign_up_answers_403_and_creates_no_account() {
    let db = full_db().await;
    let router = router(db.clone());
    let field = Uuid::new_v4().to_string();
    let cookie = Uuid::new_v4().to_string();
    let email = "forged@example.com";

    for (submitted, label) in [
        (Some(field.as_str()), "mismatched token"),
        (None, "missing token"),
    ] {
        let mut pairs = vec![
            ("display_name", "Forged"),
            ("email", email),
            ("workspace", "Forged"),
            ("password", DEMO_ADMIN_PASSWORD),
            ("password_confirmation", DEMO_ADMIN_PASSWORD),
        ];
        if let Some(field) = submitted {
            pairs.push(("csrf_token", field));
        }
        let resp = TestClient::new(&router)
            .csrf(&cookie)
            .post_form("/admin/register", form_body(&pairs))
            .await;
        assert_eq!(resp.status(), 403, "sign-up {label}");
        assert!(session_cookie_value(&resp).is_none(), "sign-up {label}");
    }
    let staff = showcase::models::Staff::filter(
        showcase::models::Staff::fields()
            .email()
            .eq(email.to_string()),
    )
    .first()
    .exec(&mut db.clone())
    .await
    .unwrap();
    assert!(staff.is_none(), "a forged sign-up must create nothing");
}

/// A forged reset request or reset answers 403 before anything runs: no link is recorded, and
/// the password stays.
#[tokio::test]
async fn forged_password_resets_answer_403_and_change_nothing() {
    let db = full_db().await;
    let router = router(db.clone());
    let field = Uuid::new_v4().to_string();
    let cookie = Uuid::new_v4().to_string();

    for (submitted, label) in [
        (Some(field.as_str()), "mismatched token"),
        (None, "missing token"),
    ] {
        for (path, mut pairs) in [
            ("/admin/forgot-password", vec![("email", DEMO_ADMIN_EMAIL)]),
            (
                "/admin/reset-password",
                vec![
                    ("token", "forged"),
                    ("password", "forged password"),
                    ("password_confirmation", "forged password"),
                ],
            ),
        ] {
            if let Some(field) = submitted {
                pairs.push(("csrf_token", field));
            }
            let resp = TestClient::new(&router)
                .csrf(&cookie)
                .post_form(path, form_body(&pairs))
                .await;
            assert_eq!(resp.status(), 403, "{path} {label}");
        }
    }
    let links = tablo::auth::AuthToken::all()
        .exec(&mut db.clone())
        .await
        .unwrap();
    assert!(links.is_empty(), "a forged request must record no link");
    let (_, login) = TestClient::new(&router)
        .log_in("admin", DEMO_ADMIN_EMAIL, DEMO_ADMIN_PASSWORD, "")
        .await;
    assert_eq!(login.status(), 303, "the password must stay");
}

/// A forged profile save answers 403 before the profile editor runs: the account keeps its name.
#[tokio::test]
async fn forged_profile_save_answers_403_and_changes_nothing() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let field = Uuid::new_v4().to_string();
    let cookie = Uuid::new_v4().to_string();

    for (submitted, label) in [
        (Some(field.as_str()), "mismatched token"),
        (None, "missing token"),
    ] {
        let mut pairs = vec![("name", "Forged"), ("email", DEMO_ADMIN_EMAIL)];
        if let Some(field) = submitted {
            pairs.push(("csrf_token", field));
        }
        let resp = client
            .csrf(&cookie)
            .post_form("/admin/profile", form_body(&pairs))
            .await;
        assert_eq!(resp.status(), 403, "profile {label}");
    }
    let staff = showcase::models::Staff::filter(
        showcase::models::Staff::fields()
            .email()
            .eq(DEMO_ADMIN_EMAIL.to_string()),
    )
    .first()
    .exec(&mut db.clone())
    .await
    .unwrap()
    .expect("the demo admin exists");
    assert_ne!(
        staff.display_name, "Forged",
        "a forged save must change nothing"
    );
}

/// A forged request for a verification link answers 403 before any link is written.
#[tokio::test]
async fn forged_verification_request_answers_403_and_mails_nothing() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let field = Uuid::new_v4().to_string();
    let cookie = Uuid::new_v4().to_string();

    for (submitted, label) in [
        (Some(field.as_str()), "mismatched token"),
        (None, "missing token"),
    ] {
        let pairs: Vec<(&str, &str)> = submitted.map(|f| ("csrf_token", f)).into_iter().collect();
        let resp = client
            .csrf(&cookie)
            .post_form("/admin/verify-email", form_body(&pairs))
            .await;
        assert_eq!(resp.status(), 403, "verification {label}");
    }
    let links = tablo::auth::AuthToken::all()
        .exec(&mut db.clone())
        .await
        .unwrap();
    assert!(links.is_empty(), "a forged request must record no link");
}

/// A forged tenant registration answers 403 before the registrar runs: no workspace is created.
#[tokio::test]
async fn forged_tenant_registration_answers_403_and_creates_nothing() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let field = Uuid::new_v4().to_string();
    let cookie = Uuid::new_v4().to_string();
    let workspaces = || async {
        showcase::models::Workspace::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .len()
    };
    let before = workspaces().await;

    for (submitted, label) in [
        (Some(field.as_str()), "mismatched token"),
        (None, "missing token"),
    ] {
        let mut pairs = vec![("name", "Forged")];
        if let Some(field) = submitted {
            pairs.push(("csrf_token", field));
        }
        let resp = client
            .csrf(&cookie)
            .post_form("/admin/register-tenant", form_body(&pairs))
            .await;
        assert_eq!(resp.status(), 403, "tenant registration {label}");
    }
    assert_eq!(
        workspaces().await,
        before,
        "a forged registration must create nothing"
    );
}

/// A forged logout POST is refused and leaves the session usable.
///
/// Both a mismatched token and a missing `csrf_token` field must 403. The
/// regression it catches is a logout that deletes the session row before
/// verifying the token, or not at all: the presented session must survive the
/// forged pair, which the follow-up authenticated GET proves.
#[tokio::test]
async fn forged_logout_answers_403_and_keeps_the_session() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let field = Uuid::new_v4().to_string();
    let cookie = Uuid::new_v4().to_string();

    for (submitted, label) in [
        (Some(field.as_str()), "mismatched token"),
        (None, "missing token"),
    ] {
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        if let Some(field) = submitted {
            pairs.push(("csrf_token", field));
        }
        let resp = client
            .csrf(&cookie)
            .post_form("/admin/logout", form_body(&pairs))
            .await;
        assert_eq!(
            resp.status(),
            403,
            "logout {label}: a forged POST must 403, got {}",
            resp.status()
        );
    }
    assert_eq!(
        client.get("/admin/posts").await.status(),
        200,
        "a forged logout must leave the session usable"
    );
}
