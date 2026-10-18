//! Password sign-in through the panel: the throttle, the body cap, the expired sessions a login
//! sweeps, and which panel's sessions a runtime endpoint admits.

use jiff::Timestamp;
use tablo::{
    Auth, Panel, RouterBuilderPanelExt,
    auth::{AdminUser, AuthSession, PanelUser, create_admin, guard, require_user},
};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Body, RemoteAddr, Router, RouterBuilderDiscoverExt, TrailingSlash},
};
use uuid::Uuid;

use crate::common::{memory_db, mount, set_cookie_header};

const SESSION_COOKIE: &str = "__Host-session";

/// A database holding ada, whose password is `opensesame`.
async fn db_with_ada() -> Db {
    let mut db = memory_db(toasty::models!(AdminUser, AuthSession)).await;
    create_admin(&mut db, "ada@example.com", "opensesame", "Ada")
        .await
        .expect("seed ada");
    db
}

fn password_panel(db: Db) -> Router {
    mount(db, Panel::new("admin").auth(Auth::password())).expect("panel builds")
}

/// A login POST to `{prefix}/login` with `body`, carrying the CSRF pair.
fn login_request(prefix: &str, body: &str) -> http::Request<Body> {
    let csrf = Uuid::new_v4().to_string();
    http::Request::builder()
        .method(http::Method::POST)
        .uri(format!("{prefix}/login"))
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(
            http::header::COOKIE,
            format!("{}={csrf}", tablo::csrf::COOKIE_NAME),
        )
        .body(Body::from(format!("{body}&csrf_token={csrf}")))
        .unwrap()
}

fn ada_with(password: &str) -> String {
    format!("email=ada@example.com&password={password}")
}

/// Ada's sign-in from `client` with `password`, answering its status.
async fn sign_in_from(router: &Router, client: [u8; 4], password: &str) -> http::StatusCode {
    let mut request = login_request("/admin", &ada_with(password));
    request
        .extensions_mut()
        .insert(RemoteAddr(std::net::SocketAddr::from((client, 4242))));
    router.handle(request).await.status()
}

/// The default throttle allows five attempts per login and address: a guesser elsewhere does not
/// lock the owner out, and the owner's sign-in clears only the owner's count.
#[tokio::test]
async fn a_sign_in_clears_its_own_count_and_no_other() {
    const OWNER: [u8; 4] = [192, 0, 2, 1];
    const GUESSER: [u8; 4] = [198, 51, 100, 7];
    let router = password_panel(db_with_ada().await);
    for _ in 0..5 {
        assert_eq!(
            sign_in_from(&router, GUESSER, "wrong").await,
            http::StatusCode::FORBIDDEN
        );
    }
    for _ in 0..2 {
        for _ in 0..4 {
            sign_in_from(&router, OWNER, "wrong").await;
        }
        assert_eq!(
            sign_in_from(&router, OWNER, "opensesame").await,
            http::StatusCode::SEE_OTHER,
            "the owner's fifth attempt signs in, and clears the owner's count"
        );
    }
    assert_eq!(
        sign_in_from(&router, GUESSER, "opensesame").await,
        http::StatusCode::FORBIDDEN,
        "the guesser's count stays"
    );
}

/// A login body past the credential cap answers 413 and starts no session; a normal one signs in.
#[tokio::test]
async fn an_oversized_login_post_is_refused() {
    let router = password_panel(db_with_ada().await);
    let oversized = format!("email={}&password=opensesame", "a".repeat(1024 * 1024));
    let response = router.handle(login_request("/admin", &oversized)).await;
    assert_eq!(response.status(), http::StatusCode::PAYLOAD_TOO_LARGE);
    assert!(set_cookie_header(&response, SESSION_COOKIE).is_none());

    let response = router
        .handle(login_request("/admin", &ada_with("opensesame")))
        .await;
    assert_eq!(response.status(), http::StatusCode::SEE_OTHER);
    assert!(set_cookie_header(&response, SESSION_COOKIE).is_some());
}

/// A login sweeps the expired sessions, whoever owns them, and leaves the live ones.
#[tokio::test]
async fn a_login_sweeps_every_expired_session() {
    let mut db = db_with_ada().await;
    let ada = AdminUser::get_by_email(&mut db, "ada@example.com")
        .await
        .unwrap();
    let grace = create_admin(&mut db, "grace@example.com", "opensesame", "Grace")
        .await
        .unwrap();
    let expired: Timestamp = "2000-01-01T00:00:00Z".parse().unwrap();
    let live: Timestamp = "2100-01-01T00:00:00Z".parse().unwrap();
    for (token_hash, user, expires_at) in [
        ("expired-mine", &ada, expired),
        ("live-mine", &ada, live),
        ("expired-other", &grace, expired),
    ] {
        toasty::create!(AuthSession {
            token_hash: token_hash.to_string(),
            user_id: user.user_id(),
            panel: "/admin".to_string(),
            expires_at,
            created_at: Timestamp::now(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }

    let router = password_panel(db.clone());
    let response = router
        .handle(login_request("/admin", &ada_with("opensesame")))
        .await;
    assert_eq!(response.status(), http::StatusCode::SEE_OTHER);
    let mut left: Vec<String> = AuthSession::all()
        .exec(&mut db)
        .await
        .unwrap()
        .into_iter()
        .map(|session| session.token_hash)
        // The login's own session is a hex digest; the seeded rows carry a dash.
        .filter(|hash| hash.contains('-'))
        .collect();
    left.sort();
    assert_eq!(left, ["live-mine"]);
}

/// A logged-out request reaches the login path only through the login methods: an app's own PUT
/// there stops at the gate.
#[tokio::test]
async fn only_the_login_methods_pass_the_gate_at_the_login_path() {
    fn app_put(cx: &Cx, _body: Body) -> topcoat::router::RouteFuture<'_> {
        Box::pin(async move {
            topcoat::router::response::IntoResponse::into_response("app route ran", cx)
        })
    }
    let router = Router::builder()
        .discover()
        .app_context(db_with_ada().await)
        .panel(Panel::new("admin").auth(Auth::password()))
        .expect("panel builds")
        .route(topcoat::router::RouteFn::new(
            http::Method::PUT,
            "/admin/login",
            app_put,
        ))
        .build();
    let put = http::Request::builder()
        .method(http::Method::PUT)
        .uri("/admin/login")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router.handle(put).await.status(),
        http::StatusCode::UNAUTHORIZED
    );
    let get = http::Request::builder()
        .uri("/admin/login")
        .body(Body::empty())
        .unwrap();
    assert_eq!(router.handle(get).await.status(), http::StatusCode::OK);
}

/// A shard the portal declares, rendering for whoever its guard admits.
#[topcoat::runtime::shard("/_topcoat/runtime/shards/tablo-it-portal-report")]
async fn portal_report(cx: &Cx) -> topcoat::Result<impl topcoat::view::View> {
    guard(cx)?;
    Ok(topcoat::view::view! { "report" })
}

const PORTAL_REPORT: &str = "/_topcoat/runtime/shards/tablo-it-portal-report";

/// Revokes the signed-in user's sessions from the admin panel, as an app's own password change
/// does.
fn revoke_own_sessions(cx: &Cx, _body: Body) -> topcoat::router::RouteFuture<'_> {
    Box::pin(async move {
        let id = require_user::<AdminUser>(cx)?.user_id();
        tablo::auth::revoke_sessions_for_user(cx, &id).await?;
        topcoat::router::response::IntoResponse::into_response("revoked", cx)
    })
}

/// Two password panels over ada's account, the portal declaring `portal_report`.
async fn two_panels(trailing_slash: TrailingSlash) -> Router {
    Router::builder()
        .trailing_slash(trailing_slash)
        .discover()
        .app_context(db_with_ada().await)
        .panel(Panel::new("admin"))
        .expect("admin builds")
        .panel(Panel::new("portal").endpoint(portal_report))
        .expect("portal builds")
        .route(topcoat::router::RouteFn::new(
            http::Method::POST,
            "/admin/it-revoke",
            revoke_own_sessions,
        ))
        .build()
}

/// Signs ada in on the panel at `prefix`, answering the session cookie's `name=value`.
async fn session_on(router: &Router, prefix: &str) -> String {
    let response = router
        .handle(login_request(prefix, &ada_with("opensesame")))
        .await;
    assert_eq!(
        response.status(),
        http::StatusCode::SEE_OTHER,
        "{prefix} signs in"
    );
    let cookie = set_cookie_header(&response, SESSION_COOKIE).expect("a session cookie");
    cookie.split(';').next().unwrap().to_string()
}

/// A shard request carrying `cookie`.
fn shard_request(path: &str, cookie: &str) -> http::Request<Body> {
    http::Request::builder()
        .method(http::Method::POST)
        .uri(path)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::COOKIE, cookie)
        .body(Body::from(r#"{"args":[]}"#))
        .unwrap()
}

/// A shard a panel declares answers only that panel's sessions, at its path and at its
/// trailing-slash twin: a session the admin panel signed in is no session on the portal's shard,
/// as it is none on the portal's pages (ADR-0013).
#[tokio::test]
async fn a_declared_shard_refuses_another_panels_session() {
    for (trailing_slash, path) in [
        (TrailingSlash::default(), PORTAL_REPORT.to_string()),
        (TrailingSlash::Serve, format!("{PORTAL_REPORT}/")),
    ] {
        let router = two_panels(trailing_slash).await;
        let admin = session_on(&router, "/admin").await;
        let response = router.handle(shard_request(&path, &admin)).await;
        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED, "{path}");
        let portal = session_on(&router, "/portal").await;
        let response = router.handle(shard_request(&path, &portal)).await;
        assert_eq!(response.status(), http::StatusCode::OK, "{path}");
    }
}

/// A shard no panel declares answers every panel's sessions: the live toaster serves them all.
#[tokio::test]
async fn an_undeclared_shard_admits_every_panels_session() {
    let router = two_panels(TrailingSlash::default()).await;
    for prefix in ["/admin", "/portal"] {
        let cookie = session_on(&router, prefix).await;
        let response = router
            .handle(shard_request(
                "/_topcoat/runtime/shards/tablo-live-toaster",
                &cookie,
            ))
            .await;
        assert_ne!(
            response.status(),
            http::StatusCode::UNAUTHORIZED,
            "{prefix}"
        );
    }
}

/// A revocation on one panel ends the user's sessions on every panel that signs in the same
/// users, as the profile's own password change does: the credential they share changed.
#[tokio::test]
async fn revoking_on_a_panel_ends_the_sessions_of_panels_sharing_its_users() {
    let router = two_panels(TrailingSlash::default()).await;
    let admin = session_on(&router, "/admin").await;
    let portal = session_on(&router, "/portal").await;
    let revoke = http::Request::builder()
        .method(http::Method::POST)
        .uri("/admin/it-revoke")
        .header(http::header::COOKIE, &admin)
        .body(Body::empty())
        .unwrap();
    assert_eq!(router.handle(revoke).await.status(), http::StatusCode::OK);
    let response = router.handle(shard_request(PORTAL_REPORT, &portal)).await;
    assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
}
