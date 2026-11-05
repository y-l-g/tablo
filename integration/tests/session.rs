//! A panel's sessions beside a host app's own Topcoat sessions on one router.

use std::time::Duration;

use tablo::{
    Auth, Page, Panel, RouterBuilderPanelExt,
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
        Ok(view! { cx => "Dashboard" })
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
    host_routes(Router::builder().discover().with(db.clone()), host_lifetime)
        .panel(Panel::new("admin").auth(Auth::password()).home::<Home>())
        .expect("the panel mounts")
        .build()
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
