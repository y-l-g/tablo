//! A panel behind `Auth::external`, over a host app's own users and sessions.

use std::time::Duration;

use http::header::LOCATION;
use tablo::{
    Auth, DeclarationErrorKind, EmailVerification, ExternalAuth, GlobalSearch, NoForm, Panel,
    PasswordAuth, PasswordReset, PasswordSignUp, Resource, ResourceDef, RouterBuilderPanelExt,
    TwoFactor,
    auth::{AdminUser, AuthSession, SESSION_COOKIE},
    lens,
};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Body, Router, RouterBuilderDiscoverExt, response::Response},
};
use uuid::Uuid;

use crate::common::{
    TestClient, body_string,
    host::{
        Employee, HOST_COOKIE, HostSession, current_employee, employee, host_login, host_routes,
    },
    mount, refusal, response_cookies, test_db,
};

const SEARCH_SHARD: &str = "/_topcoat/runtime/shards/tablo-search";
const SEARCH_ARGS: &str = r#"{"args":["/admin","ledger"],"signals":{}}"#;

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

fn staff_only(cx: &Cx, _ability: tablo::Ability<'_, Book>) -> bool {
    tablo::auth::user::<Employee>(cx).is_some_and(|employee| employee.admin)
}

struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = NoForm<Book>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy_fn(staff_only)
            .record_title(lens!(Book.title))
            .global_search(GlobalSearch::new())
    }
}

struct AppSession;

impl ExternalAuth for AppSession {
    type User = Employee;

    async fn current_user(&self, cx: &Cx) -> topcoat::Result<Option<Employee>> {
        current_employee(cx).await
    }

    fn login_url(&self, _cx: &Cx, next: &str) -> String {
        format!("/login?return_to={next}")
    }

    fn logout_url(&self, _cx: &Cx) -> String {
        "/goodbye".to_string()
    }
}

async fn db() -> Db {
    test_db(toasty::models!(
        AdminUser,
        AuthSession,
        Book,
        Employee,
        HostSession
    ))
    .await
}

fn panel() -> Panel {
    Panel::new("admin")
        .auth(Auth::external(AppSession))
        .resource::<BookResource>()
}

fn router(db: &Db) -> Router {
    host_routes(
        Router::builder().discover().with(db.clone()),
        Duration::from_hours(24),
    )
    .panel(panel())
    .expect("the panel mounts")
    .build()
}

fn location(response: &Response<Body>) -> &str {
    response
        .headers()
        .get(LOCATION)
        .expect("a redirect")
        .to_str()
        .expect("a text location")
}

async fn session_rows(db: &Db) -> Vec<AuthSession> {
    AuthSession::all()
        .exec(&mut db.clone())
        .await
        .expect("the read succeeds")
}

/// `client` after following the panel's login route, with the panel session it started.
async fn enter<'a>(client: &TestClient<'a>) -> TestClient<'a> {
    let entered = client.get("/admin/login").await;
    assert_eq!(entered.status(), 303);
    client.cookies(&response_cookies(&entered))
}

/// An anonymous page load reaches the app's login page, which can bring the visitor back to it.
#[tokio::test]
async fn an_anonymous_visitor_goes_to_the_apps_login_page() {
    let db = db().await;
    let router = router(&db);
    let visitor = TestClient::new(&router);

    let page = visitor.get("/admin/books").await;
    assert_eq!(page.status(), 307);
    let login = location(&page).to_string();
    assert_eq!(login, "/admin/login?next=%2Fadmin%2Fbooks");

    let sent = visitor.get(&login).await;
    assert_eq!(sent.status(), 307);
    let target = location(&sent)
        .strip_prefix("/login?return_to=")
        .expect("the app's login page");
    let back: String = form_urlencoded::parse(format!("back={target}").as_bytes())
        .find(|(key, _)| key == "back")
        .expect("the query holds it")
        .1
        .into_owned();
    assert_eq!(back, login);
    assert!(session_rows(&db).await.is_empty());
}

/// The app's user enters without a password, lands on `next`, and the app's session stands.
#[tokio::test]
async fn the_apps_signed_in_user_enters_the_panel() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = router(&db);
    let browser = host_login(&TestClient::new(&router), &ada).await;

    let entered = browser.get("/admin/login?next=%2Fadmin%2Fbooks").await;

    assert_eq!(entered.status(), 303);
    assert_eq!(location(&entered), "/admin/books");
    let cookies = response_cookies(&entered);
    assert!(cookies.iter().any(|(name, _)| name == SESSION_COOKIE));
    assert!(cookies.iter().all(|(name, _)| name != HOST_COOKIE));
    let rows = session_rows(&db).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].user_id, ada.id.to_string());
    assert_eq!(rows[0].panel, "/admin");
    let browser = browser.cookies(&cookies);
    assert_eq!(browser.get("/admin/books").await.status(), 200);
    assert_eq!(body_string(browser.get("/app/me").await).await, "Ada");
}

#[tokio::test]
async fn an_offsite_next_lands_on_the_panel() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = router(&db);
    let browser = host_login(&TestClient::new(&router), &ada).await;

    let entered = browser
        .get("/admin/login?next=https%3A%2F%2Fevil.example%2F")
        .await;

    assert_eq!(location(&entered), "/admin");
}

/// A session presented before the sign-in cannot be replayed after it.
#[tokio::test]
async fn entering_replaces_the_session_the_request_presents() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = router(&db);
    let browser = enter(&host_login(&TestClient::new(&router), &ada).await).await;

    enter(&browser).await;

    assert_eq!(session_rows(&db).await.len(), 1);
}

/// Signing out of the app ends the panel on its next request, pages and runtime path alike.
#[tokio::test]
async fn signing_out_of_the_app_ends_the_panel_session() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = router(&db);
    let browser = enter(&host_login(&TestClient::new(&router), &ada).await).await;
    assert_eq!(
        browser
            .call_procedure(SEARCH_SHARD, SEARCH_ARGS)
            .await
            .status(),
        200
    );

    let out = browser.post_form("/app/logout", String::new()).await;
    let browser = browser.cookies(&response_cookies(&out));

    assert_eq!(
        browser
            .call_procedure(SEARCH_SHARD, SEARCH_ARGS)
            .await
            .status(),
        401
    );
    assert!(session_rows(&db).await.is_empty());
    assert_eq!(browser.get("/admin/books").await.status(), 307);
}

/// A panel session never outlives the app account it started for: another sign-in on the app
/// reads as anonymous until the panel starts a session for the new user.
#[tokio::test]
async fn another_app_account_does_not_inherit_the_panel_session() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let grace = employee(&db, "Grace", true).await;
    let router = router(&db);
    let browser = enter(&host_login(&TestClient::new(&router), &ada).await).await;

    let browser = host_login(&browser, &grace).await;

    assert_eq!(browser.get("/admin/books").await.status(), 307);
    let browser = enter(&browser).await;
    assert_eq!(browser.get("/admin/books").await.status(), 200);
    let rows = session_rows(&db).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].user_id, grace.id.to_string());
}

/// An app user without panel access gets no panel session, and no way in by asking again.
#[tokio::test]
async fn an_app_user_without_panel_access_is_refused() {
    let db = db().await;
    let bob = employee(&db, "Bob", false).await;
    let router = router(&db);
    let browser = host_login(&TestClient::new(&router), &bob).await;

    let refused = browser.get("/admin/login").await;

    assert_eq!(refused.status(), 403);
    assert!(
        response_cookies(&refused)
            .iter()
            .all(|(name, _)| name != SESSION_COOKIE)
    );
    assert!(body_string(refused).await.contains(r#"href="/goodbye""#));
    assert!(session_rows(&db).await.is_empty());
    assert_eq!(browser.get("/admin/books").await.status(), 307);
}

/// The shell's Sign out ends the panel's session, keeps the app's, and leaves for the app.
#[tokio::test]
async fn signing_out_of_the_panel_lands_on_the_app() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = router(&db);
    let browser = enter(&host_login(&TestClient::new(&router), &ada).await).await;

    let out = browser.submit("/admin/logout", "").await;

    assert_eq!(out.status(), 303);
    assert_eq!(location(&out), "/goodbye");
    assert!(session_rows(&db).await.is_empty());
    assert_eq!(body_string(browser.get("/app/me").await).await, "Ada");
}

/// No password signs in, however the form is posted.
#[tokio::test]
async fn the_login_path_takes_no_post() {
    let db = db().await;
    let router = router(&db);

    let posted = TestClient::new(&router)
        .submit("/admin/login", "email=Ada&password=secret")
        .await;

    assert_eq!(posted.status(), 405);
    assert!(session_rows(&db).await.is_empty());
}

struct AdminSession;

impl ExternalAuth for AdminSession {
    type User = AdminUser;

    async fn current_user(&self, _cx: &Cx) -> topcoat::Result<Option<AdminUser>> {
        Ok(None)
    }

    fn login_url(&self, _cx: &Cx, _next: &str) -> String {
        "/login".to_string()
    }
}

/// What needs Tablo to own the credentials cannot mount beside external auth.
#[tokio::test]
async fn the_mount_refuses_the_features_that_own_credentials() {
    let db = db().await;
    let auth = Auth::external(AdminSession)
        .registration(PasswordSignUp::pending())
        .password_reset(PasswordReset::new(PasswordAuth))
        .email_verification(EmailVerification::new(PasswordAuth))
        .two_factor(TwoFactor::new());

    let mistakes = refusal(mount(db, Panel::new("admin").auth(auth)));

    let features: Vec<_> = mistakes
        .iter()
        .filter_map(|mistake| match mistake.kind {
            DeclarationErrorKind::ExternalAuthFeature { feature } => Some(feature),
            _ => None,
        })
        .collect();
    assert_eq!(
        features,
        [
            "Auth::registration",
            "Auth::password_reset",
            "Auth::email_verification",
            "Auth::two_factor",
        ]
    );
}
