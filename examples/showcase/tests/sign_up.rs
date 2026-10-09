//! The sign-up page: the showcase's `StaffAuth` registers a member of staff with a workspace of
//! their own, signs them in, and refuses a submission field by field.

use http::header::LOCATION;
use showcase::models::{DEMO_ADMIN_EMAIL, Seat, Staff, Workspace};
use tablo::{
    auth::{AuthSession, verify_password},
    testing::{field_error, rows},
};
use toasty::Db;

use crate::common::{
    TestClient, body_string, demo_client, form_body, full_db, input_value, login_next,
    routers::router_for_tests as router, session_cookie_value,
};

const PASSWORD: &str = "correct horse";

fn sign_up_body(email: &str, password: &str, confirmation: &str) -> String {
    form_body(&[
        ("display_name", "Ada Lovelace"),
        ("email", email),
        ("workspace", "Engines"),
        ("password", password),
        ("password_confirmation", confirmation),
    ])
}

async fn staff_count(db: &Db) -> usize {
    Staff::all().exec(&mut db.clone()).await.unwrap().len()
}

async fn workspace_count(db: &Db) -> usize {
    Workspace::all().exec(&mut db.clone()).await.unwrap().len()
}

/// A sign-up writes the member of staff, their workspace and their seat in one go, stores only
/// the password's hash, and signs them in: the session acts for the new workspace, whose posts
/// list is empty.
#[tokio::test]
async fn a_sign_up_creates_the_account_and_its_workspace_and_signs_it_in() {
    let db = full_db().await;
    let router = router(db.clone());

    let response = TestClient::new(&router)
        .submit(
            "/admin/register",
            &sign_up_body("ada@example.com", PASSWORD, PASSWORD),
        )
        .await;

    assert_eq!(response.status(), 303);
    assert_eq!(response.headers().get(LOCATION).unwrap(), "/admin");
    let staff = Staff::filter(Staff::fields().email().eq("ada@example.com".to_string()))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the sign-up writes the member of staff");
    assert_eq!(staff.display_name, "Ada Lovelace");
    assert!(staff.active);
    assert!(verify_password(PASSWORD, Some(&staff.password_hash)));
    let seats = Seat::filter(Seat::fields().staff_id().eq(staff.id))
        .exec(&mut db.clone())
        .await
        .unwrap();
    assert_eq!(seats.len(), 1);
    let workspace = Workspace::filter(Workspace::fields().id().eq(seats[0].workspace_id))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the seat names the new workspace");
    assert_eq!(workspace.name, "Engines");

    let session = session_cookie_value(&response).expect("the sign-up starts a session");
    let sessions = AuthSession::filter(AuthSession::fields().user_id().eq(staff.id.to_string()))
        .exec(&mut db.clone())
        .await
        .unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].panel, "/admin");
    let client = TestClient::new(&router).cookie(tablo::testing::SESSION_COOKIE, &session);
    let posts = client.get("/admin/posts").await;
    assert_eq!(posts.status(), 200);
    assert!(rows(&body_string(posts).await).is_empty());
}

/// A refused sign-up answers 422 with each error under its field, writes nothing, and renders
/// the submitted values back except the passwords.
#[tokio::test]
async fn a_refused_sign_up_writes_nothing_and_never_echoes_a_password() {
    let db = full_db().await;
    let router = router(db.clone());
    let staff = staff_count(&db).await;
    let workspaces = workspace_count(&db).await;

    let response = TestClient::new(&router)
        .submit(
            "/admin/register",
            &sign_up_body(DEMO_ADMIN_EMAIL, "short", "shorter"),
        )
        .await;

    assert_eq!(response.status(), 422);
    assert!(session_cookie_value(&response).is_none());
    let html = body_string(response).await;
    for field in ["email", "password", "password_confirmation"] {
        assert!(field_error(&html, field).is_some(), "{field}: {html}");
    }
    assert!(field_error(&html, "display_name").is_none(), "{html}");
    assert_eq!(
        input_value(&html, "display_name").as_deref(),
        Some("Ada Lovelace")
    );
    assert!(input_value(&html, "password").is_none(), "{html}");
    assert!(!html.contains("shorter"), "{html}");
    assert_eq!(staff_count(&db).await, staff);
    assert_eq!(workspace_count(&db).await, workspaces);
}

/// The input's own rules run before the registrar: a missing field and a malformed address are
/// refused without a write.
#[tokio::test]
async fn the_inputs_own_rules_refuse_before_the_registrar_runs() {
    let db = full_db().await;
    let router = router(db.clone());
    let staff = staff_count(&db).await;

    let body = form_body(&[
        ("email", "not-an-address"),
        ("workspace", "Engines"),
        ("password", PASSWORD),
        ("password_confirmation", PASSWORD),
    ]);
    let response = TestClient::new(&router)
        .submit("/admin/register", &body)
        .await;

    assert_eq!(response.status(), 422);
    let html = body_string(response).await;
    assert!(field_error(&html, "display_name").is_some(), "{html}");
    assert!(field_error(&html, "email").is_some(), "{html}");
    assert_eq!(staff_count(&db).await, staff);
}

/// The login page links to the sign-up page and back, each carrying the validated `next`, and a
/// sign-up lands on it.
#[tokio::test]
async fn login_and_sign_up_link_each_other_and_carry_next() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = TestClient::new(&router);
    let next = form_body(&[("next", "/admin/posts")]);

    let login = body_string(client.get(&format!("/admin/login?{next}")).await).await;
    assert!(
        login.contains(&format!("href=\"/admin/register?{next}\"")),
        "{login}"
    );
    let page = client.get(&format!("/admin/register?{next}")).await;
    assert_eq!(page.status(), 200, "the sign-up page answers anonymously");
    let page = body_string(page).await;
    assert!(
        page.contains(&format!("href=\"/admin/login?{next}\"")),
        "{page}"
    );
    assert!(page.contains("action=\"/admin/register\""), "{page}");

    let body = format!(
        "{}&{next}",
        sign_up_body("grace@example.com", PASSWORD, PASSWORD)
    );
    let response = client.submit("/admin/register", &body).await;
    assert_eq!(response.status(), 303);
    assert_eq!(response.headers().get(LOCATION).unwrap(), "/admin/posts");
}

/// A key the sign-up form does not declare answers 400 and writes nothing.
#[tokio::test]
async fn an_undeclared_key_answers_400() {
    let db = full_db().await;
    let router = router(db.clone());
    let staff = staff_count(&db).await;

    let body = format!(
        "{}&active=true",
        sign_up_body("eve@example.com", PASSWORD, PASSWORD)
    );
    let response = TestClient::new(&router)
        .submit("/admin/register", &body)
        .await;

    assert_eq!(response.status(), 400);
    assert_eq!(staff_count(&db).await, staff);
}

/// The password signs in as it was typed at sign-up, outer spaces included: the sign-up stores
/// the hash of what the login compares.
#[tokio::test]
async fn the_account_signs_in_with_the_password_as_typed() {
    let db = full_db().await;
    let router = router(db.clone());
    let spaced = "  correct horse  ";

    let response = TestClient::new(&router)
        .submit(
            "/admin/register",
            &sign_up_body("spaced@example.com", spaced, spaced),
        )
        .await;
    assert_eq!(response.status(), 303);

    let (_, login) = login_next(&router, "spaced@example.com", spaced, "").await;
    assert_eq!(login.status(), 303);
}

/// A signed-in user who opens the sign-up page lands on the panel; one who posts it anyway
/// loses the session they presented, as a login rotates it.
#[tokio::test]
async fn a_signed_in_user_is_sent_to_the_panel_and_a_sign_up_rotates_the_session() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let sessions = AuthSession::all()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .len();

    let page = client.get("/admin/register").await;
    assert_eq!(page.status(), 307);
    assert_eq!(page.headers().get(LOCATION).unwrap(), "/admin");

    let response = client
        .submit(
            "/admin/register",
            &sign_up_body("rotated@example.com", PASSWORD, PASSWORD),
        )
        .await;
    assert_eq!(response.status(), 303);
    // The demo admin's row is gone and the new account's is in its place.
    assert_eq!(
        AuthSession::all()
            .exec(&mut db.clone())
            .await
            .unwrap()
            .len(),
        sessions
    );
    assert_eq!(client.get("/admin/posts").await.status(), 307);
}
