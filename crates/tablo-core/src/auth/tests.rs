use std::sync::Arc;

use jiff::Timestamp;
use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Body, response::IntoResponse},
};

use super::{
    login::{GENERIC_ERROR, login_post},
    session::{SESSION_SWEEP_BATCH, sweep_expired_sessions, token_key},
    *,
};
use crate::{
    panel::state::PanelState,
    test_support::{memory_db, tableless_db},
};

proptest::proptest! {
    /// Whatever `?next=` carries, an accepted destination is a path on this origin: one leading
    /// `/`, no `//host`, no backslash a browser reads as `/`, no control character that splits
    /// a header.
    #[test]
    fn an_accepted_next_is_a_same_origin_path(
        next in "[ \t]{0,2}[/\\a-z:.]{0,3}\\PC{0,16}[\\r\\n\\t ]{0,2}",
    ) {
        if let Some(accepted) = safe_next(&next) {
            proptest::prop_assert_eq!(accepted, next.trim());
            proptest::prop_assert!(accepted.starts_with('/') && !accepted.starts_with("//"));
            proptest::prop_assert!(!accepted.contains('\\'));
            proptest::prop_assert!(!accepted.chars().any(char::is_control));
        }
    }

    /// A path on this origin is accepted as it is.
    #[test]
    fn a_same_origin_path_is_accepted(path in "/[a-z0-9_-]{1,8}(/[a-z0-9_-]{1,8}){0,3}(\\?[a-z]{1,5}=[a-z0-9]{0,5})?") {
        proptest::prop_assert_eq!(safe_next(&path), Some(path.as_str()));
    }
}

#[tokio::test]
async fn password_hashes_verify_round_trip() {
    let phc = hash_password("correct horse battery staple")
        .await
        .expect("hash");
    assert!(phc.starts_with("$argon2id$"), "{phc}");
    assert!(verify_password("correct horse battery staple", Some(&phc)).await);
    assert!(!verify_password("wrong", Some(&phc)).await);
    assert!(!verify_password("anything", Some("not-a-phc")).await);
    // An unknown account pays a verification and never passes.
    assert!(!verify_password("anything", None).await);
}

/// A verification leaves the async worker free: on a single-threaded runtime, a task polled
/// after it finishes first.
#[tokio::test(flavor = "current_thread")]
async fn a_password_verifies_off_the_async_worker() {
    let order = std::sync::Mutex::new(Vec::new());
    tokio::join!(
        async {
            let _ = verify_password("anything", None).await;
            order.lock().unwrap().push("verify");
        },
        async {
            order.lock().unwrap().push("other");
        },
    );
    assert_eq!(*order.lock().unwrap(), ["other", "verify"]);
}

/// Hashing work past the permits waits for one, so no more runs at once than the permits allow.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hashing_runs_no_more_at_once_than_its_permits() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let permits = Arc::new(tokio::sync::Semaphore::new(2));
    let running = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    let runs = (0..8).map(|_| {
        let (running, most) = (Arc::clone(&running), Arc::clone(&most));
        password::permitted(&permits, move || {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(20));
            running.fetch_sub(1, Ordering::SeqCst);
        })
    });
    futures_util::future::join_all(runs).await;
    assert_eq!(most.load(Ordering::SeqCst), 2);
}

#[test]
fn token_keys_are_hex_encoded_sha256() {
    let token = topcoat::session::Token::random();
    let key = token_key(&token.hash());
    assert_eq!(key.len(), 64);
    assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
}

/// Declares the shipped auth models without pushing their schema, so the first
/// statement fails at the driver.
async fn schema_less_db() -> Db {
    tableless_db(toasty::models!(AdminUser, AuthSession)).await
}

/// Maps a driver failure to the opaque sign-in copy, never the driver's text or
/// the credential rejection.
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

/// A custom `Authenticator`'s own failure keeps its mapping.
#[test]
fn infrastructure_failure_keeps_an_app_error_intact() {
    let guard: topcoat::Error = topcoat::router::error::not_found().into();
    assert!(
        super::infrastructure_failure(guard).is::<topcoat::router::error::NotFoundError>(),
        "an app-authored error must keep its own mapping"
    );
}

/// Builds the `Cx` for a login POST over the password authenticator.
fn login_cx(db: Db, token: &str) -> Cx {
    login_cx_with(db, token, Auth::password())
}

/// Builds the `Cx` for a login POST over `auth`.
fn login_cx_with(db: Db, token: &str, auth: Auth) -> Cx {
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
            crate::panel::test_support::panel_state("/admin", auth),
        ))
        .request_context(parts)
        .request_context(CookieJarCell::new())
        .build()
}

/// Runs the login handler and renders the status and body a browser receives.
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

/// A driver failure during verification answers the outage copy with no driver
/// text and no credential copy.
#[tokio::test]
async fn a_driver_login_failure_does_not_echo_driver_text() {
    // The credential query fails at the driver without a pushed schema.
    let db = schema_less_db().await;

    // The same query outside the handler carries driver text.
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

/// A wrong password keeps the generic 403, never the outage copy.
#[tokio::test]
async fn a_rejected_password_still_renders_the_generic_error() {
    let mut db = schema_less_db().await;
    db.push_schema().await.expect("push schema");
    toasty::create!(AdminUser {
        email: "ada@example.com".to_string(),
        password_hash: hash_password("opensesame").await.expect("hash"),
        display_name: "Ada".to_string(),
        active: true,
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

/// Counts `verify` calls and accepts only `opensesame`.
#[derive(Clone, Default)]
struct CountingAuth(Arc<std::sync::atomic::AtomicUsize>);

impl CountingAuth {
    fn calls(&self) -> usize {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Authenticator for CountingAuth {
    type User = AdminUser;

    async fn verify(
        &self,
        _cx: &Cx,
        _login: &str,
        password: &str,
    ) -> topcoat::Result<Option<AdminUser>> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok((password == "opensesame").then(ada))
    }

    async fn find_by_id(&self, _cx: &Cx, _id: &str) -> topcoat::Result<Option<AdminUser>> {
        Ok(None)
    }
}

/// A login past its limit answers the wrong-password 403 without asking the authenticator, even
/// with the right password.
#[tokio::test]
async fn a_login_past_its_limit_is_refused_before_the_authenticator() {
    let counting = CountingAuth::default();
    let auth = Auth::custom(counting.clone())
        .throttle(LoginThrottle::new(2, std::time::Duration::from_secs(60)));
    let token = Uuid::new_v4().to_string();
    let cx = login_cx_with(schema_less_db().await, &token, auth);
    let attempt =
        |password: &str| format!("email=ada@example.com&password={password}&csrf_token={token}");
    for _ in 0..2 {
        let (status, _) = post_login(&cx, attempt("wrong")).await;
        assert_eq!(status, http::StatusCode::FORBIDDEN);
    }
    assert_eq!(counting.calls(), 2);

    let (status, _) = post_login(&cx, attempt("opensesame")).await;
    assert_eq!(status, http::StatusCode::FORBIDDEN);
    assert_eq!(counting.calls(), 2, "the authenticator is not asked");
}

/// Builds a `Db` with pushed schema and one active admin.
async fn db_with_admin(email: &str) -> Db {
    let mut db = memory_db(toasty::models!(AdminUser, AuthSession)).await;
    toasty::create!(AdminUser {
        email: email.to_string(),
        password_hash: hash_password("opensesame").await.expect("hash"),
        display_name: "Ada".to_string(),
        active: true,
        created_at: Timestamp::now(),
    })
    .exec(&mut db)
    .await
    .expect("seed admin");
    db
}

/// One sweep, as one login runs, deletes at most [`SESSION_SWEEP_BATCH`] rows.
#[tokio::test]
async fn a_sweep_deletes_at_most_a_batch() {
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

    sweep_expired_sessions(&login_cx(db.clone(), "token"))
        .await
        .expect("the sweep runs");
    let remaining = AuthSession::all().exec(&mut db).await.expect("query").len();
    assert_eq!(remaining, 1, "one expired row past the batch");
}

/// One token sweep deletes at most [`TOKEN_SWEEP_BATCH`](super::token::TOKEN_SWEEP_BATCH)
/// expired rows, and no live one.
#[tokio::test]
async fn a_token_sweep_deletes_at_most_a_batch_of_expired_rows() {
    use super::token::{TOKEN_SWEEP_BATCH, TokenPurpose, sweep};

    let mut db = memory_db(toasty::models!(AuthToken)).await;
    let expired = "2000-01-01T00:00:00Z"
        .parse::<Timestamp>()
        .expect("a past timestamp");
    let live = "2999-01-01T00:00:00Z"
        .parse::<Timestamp>()
        .expect("a future timestamp");
    let mut seed = AuthToken::create_many();
    for (index, expires_at) in (0..=TOKEN_SWEEP_BATCH)
        .map(|index| (index, expired))
        .chain([(usize::MAX, live)])
    {
        seed = seed.item(
            AuthToken::create()
                .token_hash(format!("token-{index}"))
                .purpose(TokenPurpose::PasswordReset)
                .user_id("ada")
                .panel("/admin")
                .expires_at(expires_at)
                .created_at(Timestamp::now()),
        );
    }
    seed.exec(&mut db).await.expect("seed the token rows");

    sweep(&mut db).await.expect("the sweep runs");
    let remaining = AuthToken::all().exec(&mut db).await.expect("query");
    assert_eq!(
        remaining.len(),
        2,
        "one expired row past the batch, and the live one"
    );
    assert!(remaining.iter().any(|row| row.expires_at == live));
}

/// A sweep failure answers the opaque sign-in copy with no driver text.
#[tokio::test]
async fn a_sweep_failure_maps_to_the_opaque_sign_in_copy() {
    let cx = login_cx(schema_less_db().await, "token");
    let error = sweep_expired_sessions(&cx)
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

/// A failed session delete answers the opaque copy with no driver text.
#[tokio::test]
async fn a_driver_session_delete_failure_does_not_echo_driver_text() {
    use topcoat::context::CxTestBuilder;

    let db = schema_less_db().await;

    // The same delete outside the handler carries driver text.
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

/// A user type other than the panel's.
struct Visitor;

impl PanelUser for Visitor {
    fn user_id(&self) -> String {
        "visitor".to_string()
    }

    fn display_name(&self) -> &str {
        "Visitor"
    }
}

/// Loads no one, as a [`Visitor`].
struct VisitorAuth;

impl Authenticator for VisitorAuth {
    type User = Visitor;

    async fn verify(
        &self,
        _cx: &Cx,
        _login: &str,
        _password: &str,
    ) -> topcoat::Result<Option<Visitor>> {
        Ok(None)
    }

    async fn find_by_id(&self, _cx: &Cx, _id: &str) -> topcoat::Result<Option<Visitor>> {
        Ok(None)
    }
}

fn ada() -> AdminUser {
    AdminUser {
        id: Uuid::nil(),
        email: "ada@example.com".to_string(),
        password_hash: String::new(),
        display_name: "Ada".to_string(),
        active: true,
        email_verified_at: None,
        created_at: Timestamp::UNIX_EPOCH,
    }
}

/// A request on `/admin` with `user` signed in to `panel`, served by `serving`.
fn signed_cx(user: impl PanelUser, panel: &Arc<PanelState>, serving: &Arc<PanelState>) -> Cx {
    mounted_cx(user, panel, serving, None)
}

/// [`signed_cx`] on a router that mounts `mounted`, when given.
fn mounted_cx(
    user: impl PanelUser,
    panel: &Arc<PanelState>,
    serving: &Arc<PanelState>,
    mounted: Option<Vec<Arc<PanelState>>>,
) -> Cx {
    let (parts, ()) = http::Request::builder()
        .uri("/admin/users")
        .body(())
        .unwrap()
        .into_parts();
    let builder = topcoat::context::CxTestBuilder::new();
    let builder = match mounted {
        Some(mounted) => builder.app_context(crate::panel::state::Panels(mounted)),
        None => builder,
    };
    builder
        .request_context(parts)
        .request_context(crate::panel::state::CurrentPanel(Arc::clone(serving)))
        .request_context(SignedIn {
            user: Arc::new(user),
            panel: Arc::clone(panel),
            tenant: None,
        })
        .build()
}

#[test]
fn the_user_reads_back_as_the_apps_own_type_only() {
    let admin = Arc::new(crate::panel::test_support::panel_state(
        "/admin",
        Auth::password(),
    ));
    let visitors = Arc::new(crate::panel::test_support::panel_state(
        "/visit",
        Auth::custom(VisitorAuth),
    ));
    let cx = mounted_cx(
        ada(),
        &admin,
        &admin,
        Some(vec![Arc::clone(&admin), visitors]),
    );
    assert_eq!(
        user::<AdminUser>(&cx).map(|u| u.email.as_str()),
        Some("ada@example.com")
    );
    assert!(signed_in(&cx));
    assert!(
        user::<Visitor>(&cx).is_none(),
        "another panel's type reads as nobody"
    );
    let refused = require_user::<Visitor>(&cx)
        .err()
        .expect("wrong type refused");
    assert_eq!(refused.into_response(&cx).unwrap().status(), 403);
}

/// A type no mounted panel loads can only read as nobody, so a debug build refuses it and names
/// both types.
#[cfg(debug_assertions)]
#[test]
fn a_type_no_panel_loads_panics_in_debug() {
    let admin = Arc::new(crate::panel::test_support::panel_state(
        "/admin",
        Auth::password(),
    ));
    for mounted in [None, Some(vec![Arc::clone(&admin)])] {
        let cx = mounted_cx(ada(), &admin, &admin, mounted);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            user::<Visitor>(&cx).is_none()
        }))
        .expect_err("a type no panel loads panics");
        let message = panic.downcast_ref::<String>().expect("a formatted message");
        assert!(
            message.contains(std::any::type_name::<Visitor>())
                && message.contains(std::any::type_name::<AdminUser>()),
            "the message names both types, got {message:?}"
        );
    }
}

#[test]
fn a_user_is_nobody_on_another_panel() {
    let admin = Arc::new(crate::panel::test_support::panel_state(
        "/admin",
        Auth::password(),
    ));
    let staff = Arc::new(crate::panel::test_support::panel_state(
        "/staff",
        Auth::password(),
    ));
    let cx = signed_cx(ada(), &admin, &staff);
    assert!(user::<AdminUser>(&cx).is_none());
    assert!(!signed_in(&cx));
}

#[test]
fn an_inactive_user_is_not_signed_in() {
    let panel = Arc::new(crate::panel::test_support::panel_state(
        "/admin",
        Auth::password(),
    ));
    let cx = signed_cx(
        AdminUser {
            active: false,
            ..ada()
        },
        &panel,
        &panel,
    );
    assert!(user::<AdminUser>(&cx).is_none());
    assert!(
        resolved(&cx).is_some(),
        "the logout route still reads the user"
    );
}

/// A user holding the memberships it carries, and the default tenant it names.
struct Member(Vec<Membership>, Option<uuid::Uuid>);

impl PanelUser for Member {
    fn user_id(&self) -> String {
        "member".to_string()
    }

    fn display_name(&self) -> &str {
        "Member"
    }

    fn tenants(&self) -> &[Membership] {
        &self.0
    }

    fn default_tenant(&self) -> Option<uuid::Uuid> {
        self.1
    }
}

/// A request on `/admin` signed in as a member of `tenants`, whose session
/// selected `selected`, with an optional `Tenant` override on the `Cx`.
fn member_cx(
    tenants: &[uuid::Uuid],
    selected: Option<uuid::Uuid>,
    overridden: Option<uuid::Uuid>,
) -> Cx {
    defaulting_cx(tenants, None, selected, overridden)
}

/// [`member_cx`] for a member naming `default` as their default tenant.
fn defaulting_cx(
    tenants: &[uuid::Uuid],
    default: Option<uuid::Uuid>,
    selected: Option<uuid::Uuid>,
    overridden: Option<uuid::Uuid>,
) -> Cx {
    let panel = std::sync::Arc::new(crate::panel::test_support::panel_state(
        "/admin",
        crate::Auth::password(),
    ));
    let memberships = tenants
        .iter()
        .enumerate()
        .map(|(index, tenant)| Membership::new(*tenant, format!("Tenant {index}")))
        .collect();
    let mut builder = topcoat::context::CxTestBuilder::new()
        .request_context(crate::panel::state::CurrentPanel(std::sync::Arc::clone(
            &panel,
        )))
        .request_context(SignedIn {
            user: std::sync::Arc::new(Member(memberships, default)),
            panel,
            tenant: selected,
        })
        .app_context(crate::tenancy::TenantSource(session_tenant));
    if let Some(tenant) = overridden {
        builder = builder.request_context(crate::Tenant(tenant));
    }
    builder.build()
}

#[test]
fn the_request_acts_for_the_selected_membership_else_the_first() {
    let (a, b, stranger) = (
        uuid::Uuid::from_u128(1),
        uuid::Uuid::from_u128(2),
        uuid::Uuid::from_u128(3),
    );
    assert_eq!(crate::tenant_id(&member_cx(&[a, b], None, None)), Some(a));
    assert_eq!(
        crate::tenant_id(&member_cx(&[a, b], Some(b), None)),
        Some(b)
    );
    let cx = member_cx(&[a, b], Some(b), None);
    assert_eq!(membership(&cx).map(|m| m.name.as_str()), Some("Tenant 1"));
    // A selection the user is no longer a member of falls back to the first.
    assert_eq!(
        crate::tenant_id(&member_cx(&[a, b], Some(stranger), None)),
        Some(a)
    );
    // No membership, no tenant.
    assert_eq!(crate::tenant_id(&member_cx(&[], Some(a), None)), None);
    // An override wins, and names no membership unless the user holds it.
    let cx = member_cx(&[a, b], None, Some(stranger));
    assert_eq!(crate::tenant_id(&cx), Some(stranger));
    assert_eq!(membership(&cx), None);
    let cx = member_cx(&[a, b], None, Some(b));
    assert_eq!(membership(&cx).map(|m| m.tenant), Some(b));
}

/// A user without an address has nothing to verify: verification never holds them.
#[test]
fn verification_holds_only_a_user_with_an_address() {
    struct Nameless;
    impl PanelUser for Nameless {
        fn user_id(&self) -> String {
            "nameless".to_string()
        }
        fn display_name(&self) -> &str {
            "Nameless"
        }
    }
    let auth = Auth::password().email_verification(EmailVerification::new(PasswordAuth));
    let panel = crate::panel::test_support::panel_state("/admin", auth);
    assert!(!verification::pending(&panel, &Nameless));
}

/// Before the session selects one, the request acts for the user's default tenant while they are
/// a member of it, else their first.
#[test]
fn the_default_tenant_comes_before_the_first() {
    let (a, b, stranger) = (
        uuid::Uuid::from_u128(1),
        uuid::Uuid::from_u128(2),
        uuid::Uuid::from_u128(3),
    );
    let tenant =
        |default, selected| crate::tenant_id(&defaulting_cx(&[a, b], default, selected, None));
    assert_eq!(tenant(Some(b), None), Some(b));
    assert_eq!(tenant(Some(b), Some(a)), Some(a), "a selection wins");
    assert_eq!(tenant(Some(stranger), None), Some(a));
}
