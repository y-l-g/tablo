//! Password sign-in through the panel: the throttle, the body cap, the expired sessions a login
//! sweeps, and which panel's sessions a runtime endpoint admits.

use jiff::Timestamp;
use tablo::{
    Auth, LoginThrottle, Panel, PanelUser, RouterBuilderPanelExt,
    auth::{AdminUser, AuthSession, create_admin, guard, require_user, session_cookie},
};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Body, RemoteAddr, Router, RouterBuilderDiscoverExt, TrailingSlash},
    session::Token,
};
use uuid::Uuid;

use crate::common::{mount, set_cookie_header, test_db};

/// A database holding ada, whose password is `opensesame`.
async fn db_with_ada() -> Db {
    let mut db = test_db(toasty::models!(AdminUser, AuthSession)).await;
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

/// The account throttle counts one login across addresses: a guesser who rotates addresses runs
/// out, and the owner waits with them.
#[tokio::test]
async fn the_account_throttle_counts_a_login_across_addresses() {
    let auth = Auth::password()
        .account_throttle(LoginThrottle::new(6, std::time::Duration::from_hours(1)));
    let router = mount(db_with_ada().await, Panel::new("admin").auth(auth)).expect("panel builds");
    for host in 1..=6 {
        assert_eq!(
            sign_in_from(&router, [198, 51, 100, host], "wrong").await,
            http::StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        sign_in_from(&router, [192, 0, 2, 1], "opensesame").await,
        http::StatusCode::FORBIDDEN,
        "the seventh attempt is refused from a fresh address"
    );
}

/// A login body past the credential cap answers 413 and starts no session; a normal one signs in.
#[tokio::test]
async fn an_oversized_login_post_is_refused() {
    let router = password_panel(db_with_ada().await);
    let oversized = format!("email={}&password=opensesame", "a".repeat(1024 * 1024));
    let response = router.handle(login_request("/admin", &oversized)).await;
    assert_eq!(response.status(), http::StatusCode::PAYLOAD_TOO_LARGE);
    assert!(set_cookie_header(&response, &session_cookie("admin")).is_none());

    let response = router
        .handle(login_request("/admin", &ada_with("opensesame")))
        .await;
    assert_eq!(response.status(), http::StatusCode::SEE_OTHER);
    assert!(set_cookie_header(&response, &session_cookie("admin")).is_some());
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
        .with(db_with_ada().await)
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
        .with(db_with_ada().await)
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
    let cookie = set_cookie_header(&response, &session_cookie(prefix)).expect("a session cookie");
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

/// Names the panel it ran as. No panel declares it, so only the session and the page decide.
#[topcoat::runtime::shard("/_topcoat/runtime/shards/tablo-it-which-panel")]
async fn which_panel(cx: &Cx) -> topcoat::Result<impl topcoat::view::View> {
    guard(cx)?;
    let panel = tablo::url::panel(cx).unwrap_or_default();
    Ok(topcoat::view::view! {
        "ran-as:"
        (panel)
    })
}

const WHICH_PANEL: &str = "/_topcoat/runtime/shards/tablo-it-which-panel";

fn shard_from_page(cookie: &str, page: &str) -> http::Request<Body> {
    let mut request = shard_request(WHICH_PANEL, cookie);
    request.headers_mut().insert(
        http::header::REFERER,
        format!("https://example.test{page}").parse().unwrap(),
    );
    request
}

/// A page's fetch names its panel in the `Referer`: another panel's session is no session there.
#[tokio::test]
async fn a_fetch_from_a_page_runs_only_as_that_pages_panel() {
    let router = two_panels(TrailingSlash::default()).await;
    let admin = session_on(&router, "/admin").await;

    let response = router
        .handle(shard_from_page(&admin, "/admin/users?page=2"))
        .await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let body = tablo::testing::body_string(response).await;
    assert!(body.contains("ran-as:/admin"), "{body}");

    let response = router
        .handle(shard_from_page(&admin, "/portal/notes"))
        .await;
    assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);

    // A page outside every panel names none, and `/administration` is not `/admin`.
    for page in ["/billing", "/administration/x"] {
        let response = router.handle(shard_from_page(&admin, page)).await;
        assert_eq!(response.status(), http::StatusCode::OK, "{page}");
    }
}

/// The frames a run started on a socket opened at `page` answers, joined, for up to two seconds,
/// stopping at the first that carries `until`.
async fn socket_run(
    router: Router,
    page: &str,
    cookie: &str,
    headers: serde_json::Value,
    until: &str,
) -> String {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(topcoat::serve_until(listener, router, async {
        stopped.await.ok();
    }));
    let mut request = format!("ws://{addr}{page}").into_client_request().unwrap();
    request.headers_mut().insert(
        http::header::SEC_WEBSOCKET_PROTOCOL,
        http::HeaderValue::from_static("topcoat-runtime"),
    );
    request
        .headers_mut()
        .insert(http::header::COOKIE, cookie.parse().unwrap());
    let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let run = serde_json::json!({
        "run": 1,
        "method": "POST",
        "path": WHICH_PANEL,
        "headers": headers,
        "body": r#"{"args":[],"signals":{}}"#,
    });
    socket.send(Message::text(run.to_string())).await.unwrap();
    let mut frames = String::new();
    let read = async {
        while let Some(Ok(message)) = socket.next().await {
            if let Message::Text(text) = message {
                frames.push_str(text.as_str());
                if frames.contains(until) {
                    break;
                }
            }
        }
    };
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), read).await;
    stop.send(()).ok();
    drop(socket);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), server).await;
    frames
}

/// A socket's runs name no page of their own: each keeps the panel the handshake's URL named.
#[tokio::test]
async fn a_socket_runs_only_as_the_panel_of_the_page_that_opened_it() {
    let json = serde_json::json!({ "content-type": "application/json" });
    let router = two_panels(TrailingSlash::default()).await;
    let admin = session_on(&router, "/admin").await;
    let frames = socket_run(router, "/admin/users", &admin, json.clone(), "ran-as:").await;
    assert!(frames.contains("ran-as:/admin"), "{frames}");

    let router = two_panels(TrailingSlash::default()).await;
    let admin = session_on(&router, "/admin").await;
    let frames = socket_run(router, "/portal/notes", &admin, json, "status").await;
    assert!(frames.contains(r#""status":401"#), "{frames}");
}

/// The stamp reaches a socket on a router whose app installed the runtime layer itself.
#[tokio::test]
async fn a_socket_keeps_its_page_on_an_app_installed_runtime() {
    use topcoat::runtime::RouterBuilderRuntimeExt;
    let router = Router::builder()
        .runtime()
        .discover()
        .with(db_with_ada().await)
        .panel(Panel::new("admin"))
        .expect("admin builds")
        .panel(Panel::new("portal"))
        .expect("portal builds")
        .build();
    let admin = session_on(&router, "/admin").await;
    let json = serde_json::json!({ "content-type": "application/json" });
    let frames = socket_run(router, "/portal/notes", &admin, json, "status").await;
    assert!(frames.contains(r#""status":401"#), "{frames}");
}

/// A run cannot rename the socket's panel: Topcoat refuses the header outright.
#[tokio::test]
async fn a_run_cannot_name_another_page() {
    let router = two_panels(TrailingSlash::default()).await;
    let admin = session_on(&router, "/admin").await;
    let headers = serde_json::json!({
        "content-type": "application/json",
        "x-tablo-page-panel": "/admin",
    });
    let frames = socket_run(router, "/portal/notes", &admin, headers, "status").await;
    assert!(frames.contains(r#""status":400"#), "{frames}");
}

/// The inline scripts of the document at `path`.
async fn panel_marks(router: &Router, path: &str, cookie: Option<&str>) -> Vec<String> {
    let mut request = http::Request::get(path);
    if let Some(cookie) = cookie {
        request = request.header(http::header::COOKIE, cookie);
    }
    let response = router.handle(request.body(Body::empty()).unwrap()).await;
    tablo::testing::inline_scripts(&tablo::testing::body_string(response).await)
}

/// Runtime navigation keeps the socket when the next document's scripts are all loaded, and a
/// socket answers as the panel of the page that opened it. A panel's documents therefore carry a
/// script no other panel's do, so that entering a panel is a page load and a new socket.
#[tokio::test]
async fn entering_another_panel_is_a_page_load() {
    let router = two_panels(TrailingSlash::default()).await;
    let admin = session_on(&router, "/admin").await;
    let portal = session_on(&router, "/portal").await;

    let admin_login = panel_marks(&router, "/admin/login", None).await;
    let portal_login = panel_marks(&router, "/portal/login", None).await;
    assert!(
        portal_login
            .iter()
            .any(|script| !admin_login.contains(script)),
        "{portal_login:?} against {admin_login:?}",
    );
    assert!(
        admin_login
            .iter()
            .any(|script| !portal_login.contains(script)),
        "{admin_login:?} against {portal_login:?}",
    );

    // Within a panel a signed-in document agrees with the login page, so navigation stays.
    let admin_page = panel_marks(&router, "/admin/missing", Some(&admin)).await;
    assert_eq!(admin_page, admin_login);
    let portal_page = panel_marks(&router, "/portal/missing", Some(&portal)).await;
    assert_eq!(portal_page, portal_login);
}

/// Both cookies of a browser signed in to `/admin` and `/portal`, with the admin's and the
/// portal's `name=value` apart.
async fn signed_in_on_both(router: &Router) -> (String, String) {
    let admin = session_on(router, "/admin").await;
    let portal = session_on(router, "/portal").await;
    (admin, portal)
}

/// Each panel has its own cookie, so a second sign-in replaces nothing and each page runs as its
/// panel.
#[tokio::test]
async fn a_browser_stays_signed_in_on_each_panel() {
    let router = two_panels(TrailingSlash::default()).await;
    let (admin, portal) = signed_in_on_both(&router).await;
    let both = format!("{admin}; {portal}");

    for prefix in ["/admin", "/portal"] {
        let response = router
            .handle(shard_from_page(&both, &format!("{prefix}/notes")))
            .await;
        assert_eq!(response.status(), http::StatusCode::OK, "{prefix}");
        let body = tablo::testing::body_string(response).await;
        assert!(body.contains(&format!("ran-as:{prefix}")), "{body}");

        let frames = socket_run(
            router.clone(),
            &format!("{prefix}/notes"),
            &both,
            serde_json::json!({ "content-type": "application/json" }),
            "ran-as:",
        )
        .await;
        assert!(frames.contains(&format!("ran-as:{prefix}")), "{frames}");
    }
}

/// Signing out of a panel ends that panel's session alone.
#[tokio::test]
async fn signing_out_of_a_panel_leaves_the_others_session() {
    let router = two_panels(TrailingSlash::default()).await;
    let (admin, portal) = signed_in_on_both(&router).await;
    let csrf = Uuid::new_v4().to_string();

    let logout = router
        .handle(
            http::Request::post("/admin/logout")
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{admin}; {portal}; {}={csrf}", tablo::csrf::COOKIE_NAME),
                )
                .body(Body::from(format!("csrf_token={csrf}")))
                .unwrap(),
        )
        .await;
    assert_eq!(logout.status(), http::StatusCode::SEE_OTHER);
    assert!(set_cookie_header(&logout, &session_cookie("portal")).is_none());

    let both = format!("{admin}; {portal}");
    let response = router.handle(shard_from_page(&both, "/admin/notes")).await;
    assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
    let response = router.handle(shard_from_page(&both, "/portal/notes")).await;
    assert_eq!(response.status(), http::StatusCode::OK);
}

/// A request naming no panel may mean any session it holds, so holding two it runs as neither;
/// one the endpoint's declaration or a dead cookie settles still runs.
#[tokio::test]
async fn a_request_naming_no_panel_runs_only_on_a_session_nothing_disputes() {
    let router = two_panels(TrailingSlash::default()).await;
    let (admin, portal) = signed_in_on_both(&router).await;
    let both = format!("{admin}; {portal}");

    let response = router.handle(shard_request(WHICH_PANEL, &admin)).await;
    assert_eq!(response.status(), http::StatusCode::OK);

    let response = router.handle(shard_request(WHICH_PANEL, &both)).await;
    assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
    let response = router.handle(shard_from_page(&both, "/billing")).await;
    assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);

    // Only the portal declares its report, so the admin's session disputes nothing.
    let response = router.handle(shard_request(PORTAL_REPORT, &both)).await;
    assert_eq!(response.status(), http::StatusCode::OK);

    // A cookie whose session is gone proves nothing.
    let name = session_cookie("admin");
    let dead = format!("{name}={}; {portal}", Token::random().encode());
    let response = router.handle(shard_request(WHICH_PANEL, &dead)).await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let body = tablo::testing::body_string(response).await;
    assert!(body.contains("ran-as:/portal"), "{body}");
}

/// A session serves the panel that issued it, whichever cookie carries its token.
#[tokio::test]
async fn a_token_moved_to_another_panels_cookie_signs_in_no_one() {
    let router = two_panels(TrailingSlash::default()).await;
    let admin = session_on(&router, "/admin").await;
    let (_, token) = admin.split_once('=').unwrap();
    let moved = format!("{}={token}", session_cookie("portal"));

    let response = router
        .handle(shard_from_page(&moved, "/portal/notes"))
        .await;
    assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
    let response = router.handle(shard_request(WHICH_PANEL, &moved)).await;
    assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
}

/// A panel's cookie name is a cookie name, and no two prefixes share one.
#[test]
fn every_prefix_has_a_cookie_of_its_own() {
    let names: Vec<String> = ["admin", "/admin/", "/a/b", "/a_2fb", "/a-b", "/a_b", "/a.b"]
        .into_iter()
        .map(session_cookie)
        .collect();
    assert_eq!(names[0], "__Host-tablo_session_admin");
    assert_eq!(names[0], names[1], "spelled as for `Panel::new`");
    let distinct: std::collections::BTreeSet<&String> = names.iter().collect();
    assert_eq!(distinct.len(), names.len() - 1, "{names:?}");
    for name in &names {
        assert!(
            name.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte)),
            "{name}",
        );
    }
}
