//! A panel's sessions beside a host app's own Topcoat sessions on one router.

use std::time::Duration;

use jiff::Timestamp;
use tablo::{
    Auth, Page, Panel, RouterBuilderPanelExt, Sessions,
    auth::{AdminUser, AuthSession, SESSION_LIFETIME, create_admin},
};
use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    router::{Router, RouterBuilderDiscoverExt},
    view::{View, view},
};

use crate::common::{
    TestClient, body_string,
    host::{Employee, HOST_COOKIE, HostSession, employee, host_login, host_routes},
    response_cookies, test_db,
};

struct Home;

impl Page for Home {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { "Dashboard" })
    }
}

async fn db() -> Db {
    let mut db = test_db(toasty::models!(
        AdminUser,
        AuthSession,
        Employee,
        HostSession
    ))
    .await;
    create_admin(&mut db, "admin@example.com", "secret", "Admin")
        .await
        .expect("seed the admin");
    db
}

fn router(db: &Db, host_lifetime: Duration) -> Router {
    router_with(db, host_lifetime, Sessions::new())
}

fn router_with(db: &Db, host_lifetime: Duration, sessions: Sessions) -> Router {
    let mut builder = host_routes(Router::builder().discover().with(db.clone()), host_lifetime);
    builder
        .panel(
            Panel::new("admin")
                .auth(Auth::password().sessions(sessions))
                .home::<Home>(),
        )
        .expect("the panel mounts");
    builder.build()
}

const HOUR: Duration = Duration::from_hours(1);
const HALF_HOUR: Duration = Duration::from_mins(30);

/// A router whose panel sessions last an hour, and half an hour between requests.
fn timed_router(db: &Db) -> Router {
    let sessions = Sessions::new().lifetime(HOUR).idle_timeout(HALF_HOUR);
    router_with(db, SESSION_LIFETIME, sessions)
}

async fn session(db: &Db) -> Option<AuthSession> {
    AuthSession::all()
        .first()
        .exec(&mut db.clone())
        .await
        .expect("the read succeeds")
}

/// Seconds from now until `at`, negative once past.
fn seconds_until(at: Timestamp) -> i64 {
    at.duration_since(Timestamp::now()).as_secs()
}

fn near(seconds: i64, expected: Duration) -> bool {
    seconds.unsigned_abs().abs_diff(expected.as_secs()) < 60 && seconds > 0
}

async fn who(client: &TestClient<'_>) -> String {
    body_string(client.get("/app/me").await).await
}

/// A panel sign-in and sign-out leave the host's session alone, and the host's leave the panel's.
#[tokio::test]
async fn host_and_panel_sessions_do_not_replace_each_other() {
    let db = db().await;
    let ada = employee(&db, "Ada", false).await;
    let router = router(&db, SESSION_LIFETIME);
    let browser = host_login(&TestClient::new(&router), &ada).await;

    assert_eq!(browser.get("/admin").await.status(), 307);

    let (browser, signed) = browser
        .log_in("admin", "admin@example.com", "secret", "")
        .await;
    assert!(
        response_cookies(&signed)
            .iter()
            .all(|(name, _)| name != HOST_COOKIE)
    );
    assert_eq!(who(&browser).await, "Ada");
    assert_eq!(browser.get("/admin").await.status(), 200);

    let browser = host_login(&browser, &ada).await;
    assert_eq!(browser.get("/admin").await.status(), 200);

    let out = browser.submit("/admin/logout", "").await;
    assert_eq!(out.status(), 303);
    let browser = browser.cookies(&response_cookies(&out));
    assert_eq!(who(&browser).await, "Ada");
}

/// The host's session lifetime is the host's: a panel session keeps its own.
#[tokio::test]
async fn a_panel_session_ignores_the_hosts_lifetime() {
    let db = db().await;
    let router = router(&db, Duration::from_hours(24 * 90));

    TestClient::new(&router)
        .log_in("admin", "admin@example.com", "secret", "")
        .await;

    let row = AuthSession::all()
        .first()
        .exec(&mut db.clone())
        .await
        .expect("the read succeeds")
        .expect("the login recorded a session");
    let lifetime = row
        .expires_at
        .duration_since(row.created_at)
        .as_secs()
        .unsigned_abs();
    let expected = SESSION_LIFETIME.as_secs();
    assert!(lifetime.abs_diff(expected) < 60, "{lifetime}s");
}

/// A sign-in on a panel with an idle timeout records a session that ends after it, not after the
/// lifetime.
#[tokio::test]
async fn a_new_session_ends_after_the_idle_timeout() {
    let db = db().await;
    let router = timed_router(&db);

    TestClient::new(&router)
        .log_in("admin", "admin@example.com", "secret", "")
        .await;

    let row = session(&db).await.expect("the login recorded a session");
    assert!(near(seconds_until(row.expires_at), HALF_HOUR));
}

/// A request moves the end of an idle session forward, and never past the lifetime.
#[tokio::test]
async fn a_request_extends_an_idle_session_up_to_its_lifetime() {
    let db = db().await;
    let router = timed_router(&db);
    let (browser, _) = TestClient::new(&router)
        .log_in("admin", "admin@example.com", "secret", "")
        .await;
    let soon = Timestamp::now() + Duration::from_mins(2);
    AuthSession::all()
        .update()
        .expires_at(soon)
        .exec(&mut db.clone())
        .await
        .expect("the row ages");

    assert_eq!(browser.get("/admin").await.status(), 200);
    let row = session(&db).await.expect("the session stands");
    assert!(near(seconds_until(row.expires_at), HALF_HOUR));

    // Fifty minutes in, ten are left of the hour.
    let started = Timestamp::now() - Duration::from_mins(50);
    AuthSession::all()
        .update()
        .created_at(started)
        .expires_at(soon)
        .exec(&mut db.clone())
        .await
        .expect("the row ages");

    assert_eq!(browser.get("/admin").await.status(), 200);
    let row = session(&db).await.expect("the session stands");
    assert!(near(seconds_until(row.expires_at), Duration::from_mins(10)));
}

/// A session older than the panel's lifetime ends on its next request, whatever end its row
/// names: the lifetime set today holds for the sessions opened before it.
#[tokio::test]
async fn a_session_past_the_lifetime_ends() {
    let db = db().await;
    let router = timed_router(&db);
    let (browser, _) = TestClient::new(&router)
        .log_in("admin", "admin@example.com", "secret", "")
        .await;
    AuthSession::all()
        .update()
        .created_at(Timestamp::now() - Duration::from_hours(2))
        .expires_at(Timestamp::now() + Duration::from_hours(24))
        .exec(&mut db.clone())
        .await
        .expect("the row ages");

    assert_eq!(browser.get("/admin").await.status(), 307);
    assert!(session(&db).await.is_none());
}

/// Without an idle timeout a request leaves the session's end where the sign-in put it.
#[tokio::test]
async fn a_request_moves_no_end_without_an_idle_timeout() {
    let db = db().await;
    let router = router(&db, SESSION_LIFETIME);
    let (browser, _) = TestClient::new(&router)
        .log_in("admin", "admin@example.com", "secret", "")
        .await;
    let soon = Timestamp::now() + Duration::from_mins(2);
    AuthSession::all()
        .update()
        .expires_at(soon)
        .exec(&mut db.clone())
        .await
        .expect("the row ages");

    assert_eq!(browser.get("/admin").await.status(), 200);
    let row = session(&db).await.expect("the session stands");
    assert!(seconds_until(row.expires_at) < 180);
}
