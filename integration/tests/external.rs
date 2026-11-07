//! A panel behind `Auth::external`, over a host app's own users and sessions.

use std::time::Duration;

use http::header::LOCATION;
use tablo::{
    Auth, DeclarationErrorKind, EmailVerification, ExternalAuth, GlobalSearch, Impersonation,
    Membership, NoForm, Panel, PanelUser, PasswordAuth, PasswordReset, PasswordSignUp, Resource,
    ResourceDef, RouterBuilderPanelExt, Tenancy, TenantId, TwoFactor,
    auth::{AdminUser, AuthSession, session_cookie},
    lens,
    testing::{external_sign_out, form_body, refreshes_to, row_keys},
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
    assert!(
        cookies
            .iter()
            .any(|(name, _)| *name == session_cookie("admin"))
    );
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

/// A link can bring a browser back to the login route: the user's session stands.
#[tokio::test]
async fn entering_again_keeps_the_users_session() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = router(&db);
    let browser = enter(&host_login(&TestClient::new(&router), &ada).await).await;
    let session = session_rows(&db).await;

    let again = browser.get("/admin/login?next=%2Fadmin%2Fbooks").await;

    assert_eq!(location(&again), "/admin/books");
    assert!(
        response_cookies(&again)
            .iter()
            .all(|(name, _)| *name != session_cookie("admin"))
    );
    let rows = session_rows(&db).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].token_hash, session[0].token_hash);
}

/// A monitor carrying the app's cookie reads the route and leaves no session.
#[tokio::test]
async fn a_head_starts_no_session() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = router(&db);
    let browser = host_login(&TestClient::new(&router), &ada).await;

    let probed = browser.head("/admin/login").await;

    assert_eq!(probed.status(), 303);
    assert!(
        response_cookies(&probed)
            .iter()
            .all(|(name, _)| *name != session_cookie("admin"))
    );
    assert!(session_rows(&db).await.is_empty());
}

/// A session belongs to the panel that started it, so the next panel starts its own beside it.
#[tokio::test]
async fn another_panel_starts_its_own_session() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = host_routes(
        Router::builder().discover().with(db.clone()),
        Duration::from_hours(24),
    )
    .panel(panel())
    .and_then(|router| router.panel(Panel::new("staff").auth(Auth::external(AppSession))))
    .expect("the panels mount")
    .build();
    let browser = enter(&host_login(&TestClient::new(&router), &ada).await).await;

    let entered = browser.get("/staff/login").await;

    assert_eq!(entered.status(), 303);
    let mut panels: Vec<String> = session_rows(&db)
        .await
        .into_iter()
        .map(|row| row.panel)
        .collect();
    panels.sort();
    assert_eq!(panels, ["/admin", "/staff"]);
}

/// Signing out of the app shuts the panel on its next request, pages and runtime path alike, and
/// signing back in finds the panel session as it stood.
#[tokio::test]
async fn signing_out_of_the_app_shuts_the_panel() {
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
    assert_eq!(browser.get("/admin/books").await.status(), 307);

    let browser = host_login(&browser, &ada).await;
    assert_eq!(browser.get("/admin/books").await.status(), 200);
}

/// A connection keeps the cookies of its handshake: one that outlives the app's token rotation
/// reads as signed out, and leaves the browser's fresh tab its session.
#[tokio::test]
async fn a_stale_app_cookie_leaves_the_session_standing() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = router(&db);
    let stale = enter(&host_login(&TestClient::new(&router), &ada).await).await;
    let fresh = host_login(&stale, &ada).await;

    stale.post_form("/app/logout", String::new()).await;

    assert_eq!(
        stale
            .call_procedure(SEARCH_SHARD, SEARCH_ARGS)
            .await
            .status(),
        401
    );
    assert_eq!(fresh.get("/admin/books").await.status(), 200);
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
            .all(|(name, _)| *name != session_cookie("admin"))
    );
    assert_eq!(
        external_sign_out(&body_string(refused).await).as_deref(),
        Some("/goodbye")
    );
    assert!(session_rows(&db).await.is_empty());
    assert_eq!(browser.get("/admin/books").await.status(), 307);
}

/// The shell's Sign out ends the panel's session, keeps the app's, and moves on to the app by a
/// page of its own: `form-action 'self'` would stop a redirect that leaves the origin.
#[tokio::test]
async fn signing_out_of_the_panel_moves_on_to_the_app() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = router(&db);
    let browser = enter(&host_login(&TestClient::new(&router), &ada).await).await;

    let out = browser.submit("/admin/logout", "").await;

    assert_eq!(out.status(), 200);
    let page = body_string(out).await;
    assert_eq!(refreshes_to(&page).as_deref(), Some("/goodbye"));
    assert_eq!(external_sign_out(&page).as_deref(), Some("/goodbye"));
    assert!(session_rows(&db).await.is_empty());
    assert_eq!(body_string(browser.get("/app/me").await).await, "Ada");
}

/// A tab left open across a sign-out on the app can still sign out, and clears the panel's cookie.
#[tokio::test]
async fn sign_out_answers_after_the_app_signed_out() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = router(&db);
    let browser = enter(&host_login(&TestClient::new(&router), &ada).await).await;
    let out = browser.post_form("/app/logout", String::new()).await;
    let browser = browser.cookies(&response_cookies(&out));

    let out = browser.submit("/admin/logout", "").await;

    assert_eq!(out.status(), 200);
    assert!(session_rows(&db).await.is_empty());
}

/// A panel's Sign out with no session and no CSRF token still changes nothing.
#[tokio::test]
async fn sign_out_without_a_csrf_token_is_refused() {
    let db = db().await;
    let ada = employee(&db, "Ada", true).await;
    let router = router(&db);
    let browser = enter(&host_login(&TestClient::new(&router), &ada).await).await;
    let out = browser.post_form("/app/logout", String::new()).await;
    let browser = browser.cookies(&response_cookies(&out));

    let forged = browser.post_form("/admin/logout", String::new()).await;

    assert_eq!(forged.status(), 403);
    assert_eq!(session_rows(&db).await.len(), 1);
}

const ACME: Uuid = Uuid::from_u128(0xac);
const GLOBEX: Uuid = Uuid::from_u128(0x61);

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Note {
    #[key]
    #[auto]
    id: Uuid,
    tenant_id: TenantId,
    body: String,
}

struct NoteResource;

impl Resource for NoteResource {
    type Model = Note;
    type Form = NoForm<Note>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy_fn(|cx, _ability: tablo::Ability<'_, Note>| {
                tablo::auth::user::<Member>(cx).is_some()
            })
            .tenancy(Tenancy::column(Note::fields().tenant_id()))
    }
}

/// An app user with a seat in two tenants.
struct Member {
    employee: Employee,
    seats: Vec<Membership>,
}

impl PanelUser for Member {
    fn user_id(&self) -> String {
        self.employee.user_id()
    }

    fn display_name(&self) -> &str {
        self.employee.display_name()
    }

    fn can_access_panel(&self, panel: &str) -> bool {
        self.employee.can_access_panel(panel)
    }

    fn tenants(&self) -> &[Membership] {
        &self.seats
    }
}

struct MemberSession;

impl ExternalAuth for MemberSession {
    type User = Member;

    async fn current_user(&self, cx: &Cx) -> topcoat::Result<Option<Member>> {
        Ok(current_employee(cx).await?.map(|employee| Member {
            employee,
            seats: vec![
                Membership::new(ACME.into(), "Acme"),
                Membership::new(GLOBEX.into(), "Globex"),
            ],
        }))
    }

    fn login_url(&self, _cx: &Cx, next: &str) -> String {
        format!("/login?return_to={next}")
    }

    fn logout_url(&self, _cx: &Cx) -> String {
        "/goodbye".to_string()
    }
}

/// The tenant a user chose survives a trip through the login route and through a sign-out on the
/// app, and a tenant's records stay its own on the way.
#[tokio::test]
async fn the_selected_tenant_survives_entering_again() {
    let db = test_db(toasty::models!(AuthSession, Employee, HostSession, Note)).await;
    let ada = employee(&db, "Ada", true).await;
    for (tenant, body) in [(ACME, "acme note"), (GLOBEX, "globex note")] {
        toasty::create!(Note {
            tenant_id: TenantId::from(tenant),
            body: body.to_string(),
        })
        .exec(&mut db.clone())
        .await
        .expect("seed the note");
    }
    let globex = Note::filter(Note::fields().body().eq("globex note"))
        .first()
        .exec(&mut db.clone())
        .await
        .expect("the read succeeds")
        .expect("the note exists");
    let router = host_routes(
        Router::builder().discover().with(db.clone()),
        Duration::from_hours(24),
    )
    .panel(
        Panel::new("admin")
            .auth(Auth::external(MemberSession))
            .resource::<NoteResource>(),
    )
    .expect("the panel mounts")
    .build();
    let browser = enter(&host_login(&TestClient::new(&router), &ada).await).await;

    let chosen = form_body(&[("tenant", &GLOBEX.to_string())]);
    assert_eq!(browser.submit("/admin/tenant", &chosen).await.status(), 303);
    assert_eq!(browser.get("/admin/login").await.status(), 303);
    let out = browser.post_form("/app/logout", String::new()).await;
    let browser = host_login(&browser.cookies(&response_cookies(&out)), &ada).await;
    assert_eq!(browser.get("/admin/login").await.status(), 303);

    let rows = session_rows(&db).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tenant, Some(TenantId::from(GLOBEX)));
    let page = body_string(browser.get("/admin/notes").await).await;
    assert_eq!(row_keys(&page), [globex.id.to_string()]);
}

struct BrokenSession;

impl ExternalAuth for BrokenSession {
    type User = Employee;

    async fn current_user(&self, _cx: &Cx) -> topcoat::Result<Option<Employee>> {
        Err(std::io::Error::other("the provider said no").into())
    }

    fn login_url(&self, _cx: &Cx, _next: &str) -> String {
        "/login".to_string()
    }

    fn logout_url(&self, _cx: &Cx) -> String {
        "/goodbye".to_string()
    }
}

/// An app that cannot answer is no signed-out user: the panel fails closed and keeps the cause.
#[tokio::test]
async fn a_failing_app_answers_with_an_opaque_500() {
    let db = db().await;
    let router = mount(db, Panel::new("admin").auth(Auth::external(BrokenSession)))
        .expect("the panel mounts");

    let answer = TestClient::new(&router).get("/admin/login").await;

    assert_eq!(answer.status(), 500);
    assert!(!body_string(answer).await.contains("provider"));
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

    fn logout_url(&self, _cx: &Cx) -> String {
        "/goodbye".to_string()
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
        .two_factor(TwoFactor::new())
        .impersonation(Impersonation::new(|_, _: &AdminUser, _: &AdminUser| true));

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
            "Auth::impersonation",
        ]
    );
}
