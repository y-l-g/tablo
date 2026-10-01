use super::*;
use crate::panel::test_support::mount;

#[test]
fn safe_next_only_accepts_same_origin_relative_paths() {
    assert_eq!(safe_next("/admin/users"), Some("/admin/users"));
    assert_eq!(
        safe_next("/admin/posts?status=draft"),
        Some("/admin/posts?status=draft")
    );
    assert_eq!(safe_next("  /admin  "), Some("/admin"));
    for target in [
        "",
        "admin",
        "//evil.example/login",
        "https://evil.example/steal",
        "/\\evil.example",
        "/admin\r\nLocation: https://evil.example",
    ] {
        assert_eq!(safe_next(target), None, "must reject {target:?}");
    }
}

#[test]
fn password_hashes_verify_round_trip() {
    let phc = hash_password("correct horse battery staple").expect("hash");
    assert!(phc.starts_with("$argon2id$"), "{phc}");
    assert!(verify_password("correct horse battery staple", &phc));
    assert!(!verify_password("wrong", &phc));
    assert!(!verify_password("anything", "not-a-phc"));
}

#[test]
fn token_keys_are_hex_encoded_sha256() {
    let token = topcoat::session::Token::random();
    let key = token_key(&token.hash());
    assert_eq!(key.len(), 64);
    assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
}

/// A `Db` that declares the shipped auth models but never pushed their
/// schema: the tables are missing, so the first statement fails at the
/// driver — the setup GH #229's write-path tests use.
async fn schema_less_db() -> Db {
    Db::builder()
        .models(toasty::models!(AdminUser, AuthSession))
        .connect("sqlite::memory:")
        .await
        .expect("connect to in-memory sqlite")
}

/// GH #230: an auth or session operation that fails at the driver is an
/// infrastructure failure, so it carries the opaque sign-in copy — never
/// the driver's text, and never the login page's credential rejection.
#[test]
fn infrastructure_failure_maps_driver_errors_to_the_opaque_sign_in_copy() {
    let err = super::infrastructure_failure(toasty::Error::from_args(format_args!(
        "secret driver gunk: no such table"
    )));
    let rendered = err.to_string();
    assert!(
        rendered.contains(UNAVAILABLE_ERROR),
        "the opaque message must survive, got {rendered}"
    );
    assert!(
        !rendered.contains("gunk"),
        "driver text must not leak, got {rendered}"
    );
    assert!(
        !rendered.contains(GENERIC_ERROR),
        "an outage must not read as a rejected password, got {rendered}"
    );
}

/// GH #230 must not swallow an app-authored error: a custom
/// `Authenticator`'s own failure keeps its mapping, the way GH #229 keeps a
/// record hook's.
#[test]
fn infrastructure_failure_keeps_an_app_error_intact() {
    let guard: topcoat::Error = topcoat::router::error::not_found().into();
    assert!(
        super::infrastructure_failure(guard).is::<topcoat::router::error::NotFoundError>(),
        "an app-authored error must keep its own mapping"
    );
}

/// A `Cx` for a login POST: the request parts the handler reads (method,
/// content type, CSRF cookie) plus the cookie jar, over the shipped
/// password authenticator. `token` is both the CSRF cookie and the form
/// value the caller submits.
fn login_cx(db: Db, token: &str) -> Cx {
    use topcoat::{context::CxTestBuilder, cookie::CookieJarCell};

    let parts = http::Request::builder()
        .method(http::Method::POST)
        .uri("/admin/login")
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(
            http::header::COOKIE,
            format!("{}={token}", crate::csrf::COOKIE_NAME),
        )
        .body(())
        .unwrap()
        .into_parts()
        .0;
    CxTestBuilder::new()
        .app_context(db)
        .request_context(crate::panel::test_support::current_panel(
            crate::panel::test_support::panel_state("/admin", Auth::password()),
        ))
        .request_context(parts)
        .request_context(CookieJarCell::new())
        .build()
}

/// Runs the real login handler and renders the status and body a browser
/// would be handed.
async fn post_login(cx: &Cx, form: String) -> (http::StatusCode, String) {
    let response = login_post(cx, Body::from(form))
        .await
        .expect("a failed sign-in is answered by the login page");
    let status = response.status();
    let body = String::from_utf8_lossy(
        &http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes(),
    )
    .to_string();
    (status, body)
}

/// GH #230, login half: a database failure during credential verification
/// must not echo the driver's text — the property `db.rs` pins for
/// `unavailable`, reached through the real login handler — and must not
/// render the login page's credential copy either, or an outage would tell
/// the user their password was wrong.
///
/// Positive-controlled like GH #229's write-path tests: the same query is
/// asserted to carry driver text outside the handler first, so the
/// assertions below cannot pass vacuously.
#[tokio::test]
async fn a_driver_login_failure_does_not_echo_driver_text() {
    // Schema never pushed: the credential query cannot run, so the failure
    // is the driver's own.
    let db = schema_less_db().await;

    // Positive control: the same query outside the handler really does
    // carry driver text.
    let mut raw = db.clone();
    let driver = AdminUser::filter(
        AdminUser::fields()
            .email()
            .eq("ada@example.com".to_string()),
    )
    .first()
    .exec(&mut raw)
    .await
    .expect_err("the table is missing")
    .to_string();
    drop(raw);
    assert!(
        driver.contains("no such table"),
        "the control must be a driver failure, got {driver:?}"
    );

    let token = Uuid::new_v4().to_string();
    let cx = login_cx(db, &token);
    let (status, rendered) = post_login(
        &cx,
        format!("email=ada@example.com&password=opensesame&csrf_token={token}"),
    )
    .await;

    assert_eq!(
        status,
        http::StatusCode::SERVICE_UNAVAILABLE,
        "an outage is not a credential rejection"
    );
    assert!(
        rendered.contains(UNAVAILABLE_ERROR),
        "the opaque outage copy must reach the page, got {rendered:?}"
    );
    assert!(
        !rendered.contains(&driver) && !rendered.contains("no such table"),
        "driver text must not reach the response: the driver said {driver:?}, the response said {rendered:?}"
    );
    assert!(
        !rendered.contains(GENERIC_ERROR),
        "an outage must not read as a rejected password, got {rendered:?}"
    );
}

/// GH #230, the other half: a genuine credential rejection keeps the login
/// page's generic 403 — the same setup as the outage test with a working
/// store and a wrong password, so the two answers cannot be confused in
/// either direction.
#[tokio::test]
async fn a_rejected_password_still_renders_the_generic_error() {
    let mut db = schema_less_db().await;
    db.push_schema().await.expect("push schema");
    toasty::create!(AdminUser {
        email: "ada@example.com".to_string(),
        password_hash: hash_password("opensesame").expect("hash"),
        display_name: "Ada".to_string(),
        active: true,
        tenant_id: None,
        created_at: Timestamp::now(),
    })
    .exec(&mut db)
    .await
    .expect("seed admin");

    let token = Uuid::new_v4().to_string();
    let cx = login_cx(db, &token);
    let (status, rendered) = post_login(
        &cx,
        format!("email=ada@example.com&password=wrong&csrf_token={token}"),
    )
    .await;

    assert_eq!(
        status,
        http::StatusCode::FORBIDDEN,
        "a rejection is not an outage"
    );
    assert!(
        rendered.contains(GENERIC_ERROR),
        "the credential copy must survive, got {rendered:?}"
    );
    assert!(
        !rendered.contains(UNAVAILABLE_ERROR),
        "a rejected password must not read as an outage, got {rendered:?}"
    );
}

/// A router for a password-auth panel over `db`.
fn auth_router(db: Db) -> topcoat::router::Router {
    mount(db, Panel::new("admin").auth(Auth::password())).expect("panel builds")
}

/// A login POST as the router sees it: urlencoded, carrying the CSRF cookie
/// the double-submit check reads.
fn login_request(body: String, csrf: &str) -> http::Request<Body> {
    http::Request::builder()
        .method(http::Method::POST)
        .uri("/admin/login")
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(
            http::header::COOKIE,
            format!("{}={csrf}", crate::csrf::COOKIE_NAME),
        )
        .body(Body::from(body))
        .unwrap()
}

/// The `Set-Cookie` header for the session token, when the response set one.
fn session_cookie(response: &http::Response<Body>) -> Option<String> {
    response
        .headers()
        .get_all(http::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("__Host-session="))
        .map(str::to_string)
}

/// A `Db` with the shipped auth models, schema pushed, and one active admin.
async fn db_with_admin(email: &str) -> Db {
    let mut db = Db::builder()
        .models(toasty::models!(AdminUser, AuthSession))
        .connect("sqlite::memory:")
        .await
        .expect("connect to in-memory sqlite");
    db.push_schema().await.expect("push schema");
    toasty::create!(AdminUser {
        email: email.to_string(),
        password_hash: hash_password("opensesame").expect("hash"),
        display_name: "Ada".to_string(),
        active: true,
        tenant_id: None,
        created_at: Timestamp::now(),
    })
    .exec(&mut db)
    .await
    .expect("seed admin");
    db
}

/// GH #295: the login route caps its body at a credential form's size, not
/// the panel's 10 MiB form cap.
#[tokio::test]
async fn an_oversized_login_post_is_refused() {
    let db = db_with_admin("ada@example.com").await;
    let router = auth_router(db);

    let token = Uuid::new_v4().to_string();
    let oversized = format!(
        "email={}&password=opensesame&csrf_token={token}",
        "a".repeat(MAX_LOGIN_BYTES)
    );
    let resp = router.handle(login_request(oversized, &token)).await;
    assert_eq!(
        resp.status(),
        http::StatusCode::PAYLOAD_TOO_LARGE,
        "a login body over the credential cap must be refused, got {}",
        resp.status()
    );
    assert!(
        session_cookie(&resp).is_none(),
        "a refused login must not start a session"
    );

    // A normal credential POST still signs in.
    let token = Uuid::new_v4().to_string();
    let resp = router
        .handle(login_request(
            format!("email=ada@example.com&password=opensesame&csrf_token={token}"),
            &token,
        ))
        .await;
    assert_eq!(
        resp.status(),
        http::StatusCode::SEE_OTHER,
        "a normal credential POST must sign in, got {}",
        resp.status()
    );
    assert!(
        session_cookie(&resp).is_some(),
        "a successful login starts a session"
    );
}

/// GH #302: login sweeps expired session rows, whoever owns them, the
/// signing-in user's included (GH #295). A row whose token is never presented
/// again would otherwise stay in the table forever, because [`resolve`] only
/// purges a row it looks up.
#[tokio::test]
async fn login_sweeps_every_expired_session() {
    let mut db = db_with_admin("ada@example.com").await;
    let other = toasty::create!(AdminUser {
        email: "grace@example.com".to_string(),
        password_hash: hash_password("opensesame").expect("hash"),
        display_name: "Grace".to_string(),
        active: true,
        tenant_id: None,
        created_at: Timestamp::now(),
    })
    .exec(&mut db)
    .await
    .expect("seed the other admin");
    let ada = AdminUser::filter(
        AdminUser::fields()
            .email()
            .eq("ada@example.com".to_string()),
    )
    .first()
    .exec(&mut db)
    .await
    .expect("look up ada")
    .expect("ada exists");
    let expired = "2000-01-01T00:00:00Z"
        .parse::<Timestamp>()
        .expect("a past timestamp");
    let live = "2100-01-01T00:00:00Z"
        .parse::<Timestamp>()
        .expect("a future timestamp");
    for (token_hash, user_id, expires_at) in [
        ("expired-mine", ada.id.to_string(), expired),
        ("live-mine", ada.id.to_string(), live),
        ("expired-other", other.id.to_string(), expired),
    ] {
        toasty::create!(AuthSession {
            token_hash: token_hash.to_string(),
            user_id,
            panel: "/admin".to_string(),
            expires_at,
            created_at: Timestamp::now(),
        })
        .exec(&mut db)
        .await
        .expect("seed a session row");
    }

    let router = auth_router(db.clone());
    let token = Uuid::new_v4().to_string();
    let resp = router
        .handle(login_request(
            format!("email=ada@example.com&password=opensesame&csrf_token={token}"),
            &token,
        ))
        .await;
    assert_eq!(
        resp.status(),
        http::StatusCode::SEE_OTHER,
        "the login must succeed"
    );

    let mut check = db.clone();
    assert!(
        AuthSession::filter(
            AuthSession::fields()
                .token_hash()
                .eq("expired-mine".to_string())
        )
        .first()
        .exec(&mut check)
        .await
        .expect("query")
        .is_none(),
        "the signing-in user's expired session must be purged"
    );
    let mut check = db.clone();
    assert!(
        AuthSession::filter(
            AuthSession::fields()
                .token_hash()
                .eq("live-mine".to_string())
        )
        .first()
        .exec(&mut check)
        .await
        .expect("query")
        .is_some(),
        "a live session of the same user survives the purge"
    );
    let mut check = db.clone();
    assert!(
        AuthSession::filter(
            AuthSession::fields()
                .token_hash()
                .eq("expired-other".to_string())
        )
        .first()
        .exec(&mut check)
        .await
        .expect("query")
        .is_none(),
        "the sweep does not wait for the expired row's owner to sign in"
    );
}

/// The sweep is bounded: one login drops at most [`SESSION_SWEEP_BATCH`] rows,
/// so a large table cannot turn a login into an unbounded delete.
#[tokio::test]
async fn login_sweeps_at_most_a_batch() {
    let mut db = db_with_admin("ada@example.com").await;
    let ada = AdminUser::filter(
        AdminUser::fields()
            .email()
            .eq("ada@example.com".to_string()),
    )
    .first()
    .exec(&mut db)
    .await
    .expect("look up ada")
    .expect("ada exists");
    let expired = "2000-01-01T00:00:00Z"
        .parse::<Timestamp>()
        .expect("a past timestamp");
    let mut seed = AuthSession::create_many();
    for index in 0..=SESSION_SWEEP_BATCH {
        seed = seed.item(
            AuthSession::create()
                .token_hash(format!("expired-{index}"))
                .user_id(ada.id.to_string())
                .panel("/admin")
                .expires_at(expired)
                .created_at(Timestamp::now()),
        );
    }
    seed.exec(&mut db).await.expect("seed the expired rows");

    let router = auth_router(db.clone());
    let token = Uuid::new_v4().to_string();
    let resp = router
        .handle(login_request(
            format!("email=ada@example.com&password=opensesame&csrf_token={token}"),
            &token,
        ))
        .await;
    assert_eq!(
        resp.status(),
        http::StatusCode::SEE_OTHER,
        "the login must succeed"
    );

    let mut check = db.clone();
    let remaining = AuthSession::all()
        .exec(&mut check)
        .await
        .expect("query")
        .len();
    assert_eq!(
        remaining, 2,
        "one expired row past the batch, plus the session this login created"
    );
}

/// The sweep's own failure maps through the same opaque seam as the rest of the
/// session paths, so a login whose cleanup cannot run reports sign-in trouble
/// rather than driver text (GH #230).
#[tokio::test]
async fn a_sweep_failure_maps_to_the_opaque_sign_in_copy() {
    let cx = login_cx(schema_less_db().await, "token");
    let error = super::sweep_expired_sessions(&cx)
        .await
        .expect_err("a schema-less database fails the sweep");
    let rendered = error.to_string();
    assert!(
        rendered.contains(UNAVAILABLE_ERROR),
        "the opaque message must survive, got {rendered}"
    );
    assert!(
        !rendered.contains("no such table"),
        "driver text must not leak, got {rendered}"
    );
}

/// GH #230, session half: the session-row paths map through the same seam,
/// so a delete that fails at the driver answers the opaque copy too —
/// driver text stays in the log there as well.
#[tokio::test]
async fn a_driver_session_delete_failure_does_not_echo_driver_text() {
    use topcoat::context::CxTestBuilder;

    let db = schema_less_db().await;

    // Positive control: the same delete outside the handler really does
    // carry driver text.
    let mut raw = db.clone();
    let driver = AuthSession::filter(AuthSession::fields().user_id().eq("ada".to_string()))
        .delete()
        .exec(&mut raw)
        .await
        .expect_err("the table is missing")
        .to_string();
    drop(raw);
    assert!(
        driver.contains("no such table"),
        "the control must be a driver failure, got {driver:?}"
    );

    let cx = CxTestBuilder::new().app_context(db).build();
    let error = revoke_sessions_for_user(&cx, "ada")
        .await
        .expect_err("the delete must fail");

    let rendered = error.to_string();
    assert!(
        rendered.contains(UNAVAILABLE_ERROR),
        "the opaque message must survive, got {rendered:?}"
    );
    assert!(
        !rendered.contains(&driver) && !rendered.contains("no such table"),
        "driver text must not reach the response: the driver said {driver:?}, the response said {rendered:?}"
    );
}

/// An app route mounted at the login path under a method the login routes do
/// not serve. Discovery installs it in every test router; only the test below
/// requests it.
#[topcoat::router::route(PUT "/admin/login")]
async fn app_put_at_the_login_path() -> topcoat::Result<&'static str> {
    Ok("app route ran")
}

/// The login bypass admits only the methods the login routes serve: a
/// logged-out PUT at the login path is answered by the gate, so the app route
/// above never runs unauthenticated.
#[tokio::test]
async fn the_login_bypass_is_scoped_to_the_login_methods() {
    let db = db_with_admin("ada@example.com").await;
    let router = auth_router(db);

    let put = router
        .handle(
            http::Request::builder()
                .method(http::Method::PUT)
                .uri("/admin/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        put.status(),
        http::StatusCode::UNAUTHORIZED,
        "a logged-out PUT at the login path must stop at the gate"
    );

    let get = router
        .handle(
            http::Request::builder()
                .uri("/admin/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        get.status(),
        http::StatusCode::OK,
        "the login page must still answer while logged out"
    );
}
