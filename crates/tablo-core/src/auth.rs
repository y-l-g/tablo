//! Authentication — credentials, server-side sessions, and the panel gate
//! (ADR-0013, spec #127).
//!
//! A [`Panel`] is gated by default. The shipped [`PasswordAuth`]
//! verifies Argon2id PHC hashes against the [`AdminUser`] model, a session
//! cookie issued by Topcoat's token transport identifies one server-side
//! [`AuthSession`] row, and the resolved [`CurrentUser`] travels in request
//! `Cx` for pages, shards, and app code. An app with its own user table
//! implements [`Authenticator`] and swaps it in with
//! [`Panel::auth`](crate::Panel::auth); [`Auth::disabled`] is the explicit,
//! greppable opt-out for public demos.
//!
//! Sessions are always the framework's: the shipped [`AuthSession`] table maps
//! a token hash to a user id and the panel that signed the user in, so an app
//! registers it whatever authenticator it uses. A custom user model does not
//! have to be an `AdminUser`.
//!
//! Every panel on a router has its own [`Auth`]. A session belongs to the
//! panel that issued it: another panel's gate does not resolve it, and
//! [`current_user`] answers only on the panel the user signed in to.

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use jiff::Timestamp;
use toasty::Db;
use topcoat::{
    context::{Cx, try_request_context},
    router::{
        Body, Layer, LayerFuture, Next, Path, PathBuf, RouteFuture,
        error::{forbidden, redirect, unauthorized},
        request::{method, original_headers, original_method, uri},
        response::IntoResponse,
    },
    session::{self, TokenHash},
    view::{BoxView, ViewExt},
};
use uuid::Uuid;

use crate::panel::{
    Panel, panel_prefix, route_path,
    state::{CurrentPanel, PanelState, Panels, UserPanel, current, panels},
};

/// How long a session stays valid: seven days, fixed (ADR-0013).
pub const SESSION_LIFETIME: Duration = Duration::from_hours(24 * 7);

/// Max body the login route accepts: 64 KiB.
///
/// A credential form submits an email, a password, a `next` and a CSRF token,
/// all short strings; the panel's 10 MiB form cap is for multipart uploads,
/// which the login route never carries. 64 KiB leaves room for the fields plus
/// percent-encoding growth while keeping the one unauthenticated POST route
/// from buffering a megabyte-scale body.
pub(crate) const MAX_LOGIN_BYTES: usize = 64 * 1024;

/// Form field carrying the login identifier (the shipped default reads it as
/// an email address).
pub const LOGIN_FIELD: &str = "email";
/// Form field carrying the password.
pub const PASSWORD_FIELD: &str = "password";
/// Hidden form field carrying the validated post-login destination.
pub const NEXT_FIELD: &str = "next";

/// The one error every failed login renders, so accounts cannot be
/// enumerated and panel membership stays private (ADR-0013).
const GENERIC_ERROR: &str = "Invalid email or password.";

/// What a failed login attempt renders when the database behind it could not
/// answer.
///
/// [`GENERIC_ERROR`] is deliberately non-specific about *why* credentials were
/// refused, so reusing it for an outage would tell a user their password was
/// wrong while the database was down — the one place they would retry
/// uselessly. This copy names the machinery that is down instead, and like the
/// generic one it never carries driver text: that goes to the log.
const UNAVAILABLE_ERROR: &str = "Sign-in is unavailable right now. Try again shortly.";

/// A dummy Argon2id PHC string verified against when the account does not
/// exist, so unknown emails pay the same work as known ones (ADR-0013).
/// Generated with `Argon2::default()` parameters (`m=19456,t=2,p=1`).
const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$h3oXdPBVwhcgZ1OTO/PuzQ$zLrHLgIkwhqu4ZlLTfSyB8mPuL6mAtaswv/eXJ5ADO8";

/// The shipped credential model (ADR-0013): unique email, Argon2id PHC hash,
/// display name, active flag, and an optional tenant.
///
/// Register it (plus [`AuthSession`]) in the app's `Db` model list, seed one
/// row, and the default [`PasswordAuth`] works with no further wiring:
/// `toasty::models!(…, tablo_core::auth::AdminUser, tablo_core::auth::AuthSession)`.
#[derive(Debug, Clone, toasty::Model)]
pub struct AdminUser {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub email: String,
    /// Argon2id password hash in PHC string format.
    pub password_hash: String,
    pub display_name: String,
    /// `false` denies login and panel access immediately.
    pub active: bool,
    pub tenant_id: Option<Uuid>,
    pub created_at: Timestamp,
}

/// The shipped server-side session record (ADR-0013): the SHA-256 hash of the
/// client token, the user it authenticates (opaque id), the panel that signed
/// the user in, and its expiry.
///
/// The raw token is never stored; a leaked session table contains nothing a
/// client could present. Register this model alongside the app's user model.
#[derive(Debug, Clone, toasty::Model)]
pub struct AuthSession {
    /// Hex-encoded SHA-256 of the session token (the raw token stays client-side).
    #[key]
    pub token_hash: String,
    /// [`CurrentUser::id`] of the authenticated user.
    #[index]
    pub user_id: String,
    /// The prefix of the panel that signed the user in (e.g. `/admin`): the
    /// only panel the session authenticates.
    pub panel: String,
    /// Indexed for the login sweep, whose filter is a range scan on this
    /// column; an app migrating an existing table adds the index with the
    /// model's own schema change.
    #[index]
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
}

/// The erased identity resolution places in request `Cx` (ADR-0013).
///
/// `id` is an opaque string so non-UUID keys fit; read it only through
/// [`current_user`] / [`require_authenticated`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentUser {
    /// Stable, opaque user key; custom user tables may use any string.
    pub id: String,
    /// The identifier the user logged in with (the shipped default: email).
    pub login: String,
    pub display_name: String,
    /// Optional tenant carried from the user: [`tenant_id`] answers it on the
    /// user's panel.
    ///
    /// [`tenant_id`]: crate::tenant_id
    pub tenant_id: Option<Uuid>,
    /// Whether the user may enter the panel. Denied users answer 403,
    /// indistinguishable from bad credentials at login (ADR-0013).
    pub can_access_panel: bool,
}

/// The boxed future every [`Authenticator`] method returns.
pub type AuthFuture<'a, T> = Pin<Box<dyn Future<Output = topcoat::Result<T>> + Send + 'a>>;

/// The one authentication seam (ADR-0013).
///
/// The default [`PasswordAuth`] implements it against the shipped
/// [`AdminUser`] model; an app with an existing user table implements it and
/// passes the value to [`Panel::auth`](crate::Panel::auth) via
/// [`Auth::custom`]. Sessions are the framework's, so a custom implementation
/// only maps credentials to a [`CurrentUser`] and back.
///
/// Every *credential* failure must return `Ok(None)`, never a distinguishable
/// error: the login response is one generic message for all of them. An
/// infrastructure failure is not a credential verdict, so an implementation
/// that cannot reach its store may return the driver's error instead — the
/// login handler maps it to the opaque outage page, which is what
/// keeps a database outage from rendering as a rejected password. An
/// implementation's own error keeps its own mapping.
pub trait Authenticator: Send + Sync + 'static {
    /// Verify `login`/`password`, returning the user on success.
    ///
    /// Implementations must run comparable work for unknown accounts (the
    /// shipped [`PasswordAuth`] verifies a dummy hash) so timing does not leak
    /// account existence.
    fn verify<'a>(
        &'a self,
        cx: &'a Cx,
        login: &'a str,
        password: &'a str,
    ) -> AuthFuture<'a, Option<CurrentUser>>;

    /// Resolve the live session user by [`CurrentUser::id`].
    ///
    /// Loading the row each request is what makes deactivation and revocation
    /// take effect immediately; return `None` when the user does not
    /// authenticate.
    fn find_by_id<'a>(&'a self, cx: &'a Cx, id: &'a str) -> AuthFuture<'a, Option<CurrentUser>>;
}

/// Map a failed auth or session operation to the error the response carries:
/// [`crate::error::driver_failure`] with the sign-in outage copy.
///
/// A driver failure becomes an infrastructure error whose `Display` is
/// [`UNAVAILABLE_ERROR`], and the driver's own text goes to the log, never to
/// the page; anything else is app-authored (a custom [`Authenticator`]'s own
/// error) and keeps its mapping. Every auth and session path maps through
/// here, which is what keeps a database outage from reading as a rejected
/// password — and what makes the two distinguishable in the logs: a rejection
/// is `Ok(None)`, the generic 403 and no error line, while an outage logs the
/// driver's text.
fn infrastructure_failure(error: impl Into<topcoat::Error>) -> topcoat::Error {
    crate::error::driver_failure(error, UNAVAILABLE_ERROR)
}

/// The shipped default authenticator: Argon2id verification against
/// [`AdminUser`].
#[derive(Debug, Default, Clone, Copy)]
pub struct PasswordAuth;

impl PasswordAuth {
    /// Creates the shipped authenticator.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Authenticator for PasswordAuth {
    fn verify<'a>(
        &'a self,
        cx: &'a Cx,
        login: &'a str,
        password: &'a str,
    ) -> AuthFuture<'a, Option<CurrentUser>> {
        Box::pin(async move {
            let mut db = crate::db::db(cx);
            let found = AdminUser::filter(AdminUser::fields().email().eq(login.trim().to_string()))
                .first()
                .exec(&mut db)
                .await
                .map_err(infrastructure_failure)?;
            let Some(user) = found else {
                // Unknown account: pay a verification anyway (ADR-0013).
                let _ = verify_password(password, DUMMY_PASSWORD_HASH);
                return Ok(None);
            };
            if !verify_password(password, &user.password_hash) {
                return Ok(None);
            }
            Ok(Some(current_user_from(&user)))
        })
    }

    fn find_by_id<'a>(&'a self, cx: &'a Cx, id: &'a str) -> AuthFuture<'a, Option<CurrentUser>> {
        Box::pin(async move {
            let Ok(id) = Uuid::parse_str(id) else {
                return Ok(None);
            };
            let mut db = crate::db::db(cx);
            let user = AdminUser::filter(AdminUser::fields().id().eq(id))
                .first()
                .exec(&mut db)
                .await
                .map_err(infrastructure_failure)?;
            // A deactivated account stops resolving: its live sessions are
            // purged and the next request redirects to login (spec #127 US11).
            Ok(user
                .filter(|user| user.active)
                .map(|user| current_user_from(&user)))
        })
    }
}

/// Hash a password with Argon2id into a PHC string (the shipped storage
/// format). Seeds and record fns call this; it never stores the plaintext.
pub fn hash_password(password: &str) -> topcoat::Result<String> {
    use argon2::password_hash::PasswordHasher;

    argon2::Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(topcoat::Error::from)
}

/// Verify a password against a PHC hash; `false` on any malformed input.
fn verify_password(password: &str, phc: &str) -> bool {
    use argon2::password_hash::{PasswordVerifier, phc::PasswordHash};

    let Ok(parsed) = PasswordHash::new(phc) else {
        return false;
    };
    argon2::Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Map the shipped model onto the erased identity.
fn current_user_from(user: &AdminUser) -> CurrentUser {
    CurrentUser {
        id: user.id.to_string(),
        login: user.email.clone(),
        display_name: user.display_name.clone(),
        tenant_id: user.tenant_id,
        can_access_panel: user.active,
    }
}

/// The panel's authentication configuration (ADR-0013).
///
/// The default is [`Auth::password`]; [`Auth::custom`] swaps in an app-owned
/// [`Authenticator`]; [`Auth::disabled`] is the explicit fail-open opt-out.
pub enum Auth {
    /// The shipped Argon2id + [`AdminUser`] authenticator.
    Password(PasswordAuth),
    /// An app-owned authenticator over its own user table.
    Custom(Box<dyn Authenticator>),
    /// Explicit opt-out: no gate, no login routes, sessions unused.
    Disabled,
}

impl Auth {
    /// The shipped default.
    #[must_use]
    pub fn password() -> Self {
        Self::Password(PasswordAuth::new())
    }

    /// An app-owned authenticator, erased to the trait object the panel keeps.
    #[must_use]
    pub fn custom(authenticator: impl Authenticator) -> Self {
        Self::Custom(Box::new(authenticator))
    }

    /// Explicit fail-open opt-out for public demos (ADR-0013).
    #[must_use]
    pub fn disabled() -> Self {
        Self::Disabled
    }

    /// The authenticator, or `None` when auth is explicitly disabled.
    #[must_use]
    pub fn authenticator(&self) -> Option<&dyn Authenticator> {
        match self {
            Self::Password(password) => Some(password),
            Self::Custom(authenticator) => Some(&**authenticator),
            Self::Disabled => None,
        }
    }

    /// Whether the explicit opt-out is set.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        matches!(self, Self::Disabled)
    }
}

impl Default for Auth {
    fn default() -> Self {
        Self::password()
    }
}

/// Whether the request's panel requires a signed-in user.
///
/// Outside any panel — an app's own shard or page on a router that mounts
/// several — it answers whether some mounted panel does, so a check there
/// fails closed.
pub fn enforced(cx: &Cx) -> bool {
    match current(cx) {
        Some(panel) => panel.gates(),
        None => panels(cx).is_some_and(Panels::any_gates),
    }
}

/// The resolved identity for this request, if any.
///
/// This is the only read path for pages and app code; the gate places the
/// value in request `Cx` (ADR-0013). A user is the user of the panel whose
/// session signed them in: on another panel's request this is `None`.
pub fn current_user(cx: &Cx) -> Option<CurrentUser> {
    let user = try_request_context::<CurrentUser>(cx)?;
    if let Some(UserPanel(owner)) = try_request_context::<UserPanel>(cx)
        && let Some(panel) = current(cx)
        && !Arc::ptr_eq(owner, panel)
    {
        return None;
    }
    Some(user.clone())
}

/// Require an authenticated, panel-permitted user.
///
/// Answers per request kind (ADR-0013): pages redirect to the login route
/// with a same-origin-relative `next`, runtime endpoints, non-GET requests,
/// and page re-runs (marked POSTs the runtime layer rewrites into GETs)
/// answer 401, and an authenticated user without panel access answers 403.
pub fn require_authenticated(cx: &Cx) -> topcoat::Result<CurrentUser> {
    if let Some(user) = current_user(cx) {
        if user.can_access_panel {
            return Ok(user);
        }
        return Err(forbidden().into());
    }
    Err(unauthenticated_error(cx))
}

/// The error an unauthenticated request answers with, by request kind.
fn unauthenticated_error(cx: &Cx) -> topcoat::Error {
    let path = uri(cx).path();
    let page_method = matches!(*method(cx), http::Method::GET | http::Method::HEAD);
    // A page re-run reaches the gate as a rewritten GET: Topcoat's runtime
    // layer rewrites the browser's marked POST into a GET for the page's own
    // URL. The original request tells it apart from a plain page load, so a
    // logged-out re-run answers 401 like any other non-page request instead
    // of redirecting into the login page.
    let rerun = !matches!(*original_method(cx), http::Method::GET | http::Method::HEAD)
        && original_headers(cx).get(&topcoat::runtime::RUNTIME_HEADER) == Some(&RERUN_MARKER);
    if path.starts_with(RUNTIME_PREFIX) || !page_method || rerun {
        unauthorized().into()
    } else {
        redirect(login_url_with_next(cx)).into()
    }
}

/// Where the panel's login page lives: `{prefix}/login`.
fn login_url(cx: &Cx) -> String {
    format!("{}/login", panel_prefix(cx))
}

/// Where the shell's logout control posts: `{prefix}/logout`.
pub(crate) fn logout_url(cx: &Cx) -> String {
    format!("{}/logout", panel_prefix(cx))
}

/// The login URL with a validated `next` back to the requested page.
fn login_url_with_next(cx: &Cx) -> String {
    let mut url = login_url(cx);
    let request = uri(cx);
    let requested = match request.query() {
        Some(query) => format!("{}?{query}", request.path()),
        None => request.path().to_string(),
    };
    if let Some(next) = safe_next(&requested) {
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        serializer.append_pair(NEXT_FIELD, next);
        url.push('?');
        url.push_str(&serializer.finish());
    }
    url
}

/// The panel root: where a completed login lands without a `next`.
fn panel_root(cx: &Cx) -> String {
    panel_prefix(cx)
}

/// Accept only same-origin relative paths as a post-login destination
/// (ADR-0013): absolute URLs, scheme-relative `//host` targets, backslash
/// tricks, and control characters are rejected.
pub(crate) fn safe_next(next: &str) -> Option<&str> {
    let next = next.trim();
    if !next.starts_with('/') || next.starts_with("//") {
        return None;
    }
    if next.contains('\\') || next.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(next)
}

/// The validated `?next=` the login page embeds as a hidden field.
fn next_from_query(cx: &Cx) -> Option<String> {
    let query = uri(cx).query()?;
    form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == NEXT_FIELD)
        .map(|(_, value)| value.into_owned())
        .filter(|value| safe_next(value).is_some())
}

/// Hex-encode a token hash into its storage key.
fn token_key(hash: &TokenHash) -> String {
    use std::fmt::Write as _;

    let mut key = String::with_capacity(64);
    for byte in hash.iter() {
        write!(key, "{byte:02x}").expect("writing to a String cannot fail");
    }
    key
}

/// Delete the session row a token hash names, if any.
async fn delete_session(cx: &Cx, hash: &TokenHash) -> topcoat::Result<()> {
    delete_session_row(cx, &token_key(hash)).await
}

/// Delete one stored session row by its hex token-hash key.
async fn delete_session_row(cx: &Cx, key: &str) -> topcoat::Result<()> {
    let mut db = crate::db::db(cx);
    AuthSession::filter(AuthSession::fields().token_hash().eq(key.to_string()))
        .delete()
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?;
    Ok(())
}

/// Revoke every live session of `user_id` (ADR-0013).
///
/// Call this whenever a credential changes out from under live sessions —
/// password reset/change and deactivation alike. Nothing in-core calls it
/// (there is no password-change flow in the framework); sessions otherwise
/// stay valid for their full fixed lifetime, so a reset that skips this
/// leaves a stolen session usable. A password-reset flow must call it,
/// and custom `Authenticator` apps own the same obligation.
///
/// On a panel's request it revokes the sessions that panel issued, since
/// another panel's `user_id` may name someone else; outside any panel it
/// revokes `user_id`'s sessions on every panel.
pub async fn revoke_sessions_for_user(cx: &Cx, user_id: &str) -> topcoat::Result<()> {
    let mut db = crate::db::db(cx);
    let user = AuthSession::fields().user_id().eq(user_id.to_string());
    let sessions = match current(cx) {
        Some(panel) => {
            AuthSession::filter(user.and(AuthSession::fields().panel().eq(panel.prefix.clone())))
        }
        None => AuthSession::filter(user),
    };
    sessions
        .delete()
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?;
    Ok(())
}

/// How many expired session rows one sweep drops: the bound that keeps one
/// login from deleting an unbounded number of rows.
const SESSION_SWEEP_BATCH: usize = 500;

/// Drop up to [`SESSION_SWEEP_BATCH`] expired session rows, whoever owns them.
///
/// [`resolve`] purges a row when its token is looked up expired, so a row whose
/// token is never presented again stays in the table (GH #302). A successful
/// login is where the sweep runs, before the new row: a visitor who never signs
/// in does not reach it. Toasty's `Delete` carries no `LIMIT`, so the bound
/// comes from selecting the keys first, and the select rides `expires_at`'s
/// index.
async fn sweep_expired_sessions(cx: &Cx) -> topcoat::Result<()> {
    let now = Timestamp::now();
    let mut db = crate::db::db(cx);
    let expired: Vec<String> = AuthSession::filter(AuthSession::fields().expires_at().le(now))
        .limit(SESSION_SWEEP_BATCH)
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?
        .into_iter()
        .map(|row| row.token_hash)
        .collect();
    if expired.is_empty() {
        return Ok(());
    }
    // The expiry is re-read here: the select and the delete are two statements,
    // so a row whose lifetime was extended between them is no longer expired.
    AuthSession::filter(
        AuthSession::fields()
            .token_hash()
            .in_list(expired)
            .and(AuthSession::fields().expires_at().le(now)),
    )
    .delete()
    .exec(&mut db)
    .await
    .map_err(infrastructure_failure)?;
    Ok(())
}

/// The request's live session row, lazily and without touching the database
/// when no session cookie is present. An expired row is purged on the way out.
async fn session_row(cx: &Cx) -> topcoat::Result<Option<AuthSession>> {
    let Some(hash) = session::token_hash(cx).await? else {
        return Ok(None);
    };
    let key = token_key(&hash);
    let mut db = crate::db::db(cx);
    let row = AuthSession::filter(AuthSession::fields().token_hash().eq(key))
        .first()
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.expires_at <= Timestamp::now() {
        delete_session_row(cx, &row.token_hash).await?;
        return Ok(None);
    }
    Ok(Some(row))
}

/// The user `row` authenticates through `authenticator`. A row naming a user
/// who no longer authenticates (deleted or deactivated) is purged, so removal
/// is real (US11).
async fn session_user(
    cx: &Cx,
    row: &AuthSession,
    authenticator: &dyn Authenticator,
) -> topcoat::Result<Option<CurrentUser>> {
    match authenticator
        .find_by_id(cx, &row.user_id)
        .await
        .map_err(infrastructure_failure)?
    {
        Some(user) => Ok(Some(user)),
        None => {
            delete_session_row(cx, &row.token_hash).await?;
            Ok(None)
        }
    }
}

/// Resolve the request's session into `panel`'s user. A session another panel
/// issued resolves to no one here, and stays valid there.
pub(crate) async fn resolve(
    cx: &Cx,
    panel: &PanelState,
    authenticator: &dyn Authenticator,
) -> topcoat::Result<Option<CurrentUser>> {
    match session_row(cx).await? {
        Some(row) if row.panel == panel.prefix => session_user(cx, &row, authenticator).await,
        _ => Ok(None),
    }
}

/// `cx` for a request `panel`'s auth resolved to `user`.
fn signed_in(cx: &Cx, user: CurrentUser, panel: &Arc<PanelState>) -> Cx {
    cx.with_many((user, UserPanel(Arc::clone(panel))))
}

/// The layer every request under a panel's prefix runs first (ADR-0013): it
/// puts the panel on the request, then, on a gated panel, resolves the
/// session into request `Cx` and answers fail-closed when the route has no
/// permitted user.
pub(crate) struct PanelGate {
    path: PathBuf,
    panel: Arc<PanelState>,
}

impl PanelGate {
    pub(crate) fn new(panel: Arc<PanelState>) -> Self {
        Self {
            path: route_path(&panel.prefix),
            panel,
        }
    }
}

impl Layer for PanelGate {
    fn path(&self) -> Option<&Path> {
        Some(&self.path)
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let cx = cx.with(CurrentPanel(Arc::clone(&self.panel)));
            let Some(authenticator) = self.panel.auth.authenticator() else {
                return next.run(&cx, body).await;
            };
            // The login page must answer while logged out. Only the methods
            // the login routes serve (GET, its HEAD, and POST) pass: an app
            // route at the same path under another method stays gated.
            if uri(&cx).path() == login_url(&cx)
                && matches!(
                    *method(&cx),
                    http::Method::GET | http::Method::HEAD | http::Method::POST
                )
            {
                return next.run(&cx, body).await;
            }
            // The logout route must answer for any resolved user, even one
            // whose panel access was revoked after login — clearing
            // the session row + cookie must not require panel permission, or
            // the session lingers to expiry. The bypass is POST-only at the
            // exact logout path: the route table registers nothing else
            // there, and a non-POST method must not smuggle a resolved
            // identity to any handler an app might mount at the same path.
            let logout_route =
                uri(&cx).path() == logout_url(&cx) && matches!(*method(&cx), http::Method::POST);
            match resolve(&cx, &self.panel, authenticator).await? {
                Some(user) if user.can_access_panel || logout_route => {
                    next.run(&signed_in(&cx, user, &self.panel), body).await
                }
                // Authenticated but not permitted: 403, indistinguishable
                // from bad credentials at login (ADR-0013). The logout route
                // is answered above.
                Some(_) => Err(forbidden().into()),
                // Pages redirect to the login route with a validated `next`;
                // runtime endpoints, non-GET requests, and page re-runs
                // answer 401.
                None => Err(unauthenticated_error(&cx)),
            }
        })
    }
}

/// The layer over Topcoat's runtime endpoints (ADR-0013): shards and
/// procedures every panel's pages call, served at one path for all of them.
///
/// A session names the panel that issued it, so the gate resolves it through
/// that panel's auth and puts both the user and the panel on the request. A
/// request with no session answers 401 when every mounted panel is gated; with
/// an ungated panel mounted it passes on, and each panel's shard re-checks its
/// own panel's gate.
pub(crate) struct RuntimeGate {
    path: PathBuf,
}

impl RuntimeGate {
    pub(crate) fn new() -> Self {
        Self {
            path: route_path(RUNTIME_PREFIX),
        }
    }
}

impl Layer for RuntimeGate {
    fn path(&self) -> Option<&Path> {
        Some(&self.path)
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let Some(panels) = panels(cx).filter(|panels| panels.any_gates()) else {
                return next.run(cx, body).await;
            };
            let signed = match session_row(cx).await? {
                Some(row) => match panels
                    .by_prefix(&row.panel)
                    .and_then(|panel| Some((panel, panel.auth.authenticator()?)))
                {
                    Some((panel, authenticator)) => session_user(cx, &row, authenticator)
                        .await?
                        .map(|user| (Arc::clone(panel), user)),
                    None => None,
                },
                None => None,
            };
            match signed {
                Some((panel, user)) if user.can_access_panel => {
                    let cx = signed_in(cx, user, &panel).with(CurrentPanel(panel));
                    next.run(&cx, body).await
                }
                Some(_) => Err(forbidden().into()),
                None if panels.all_gate() => Err(unauthorized().into()),
                None => next.run(cx, body).await,
            }
        })
    }
}

/// What a failed login attempt renders: the generic credential rejection, or
/// the sign-in outage.
///
/// Two variants and no more, each with its own copy and status, so a failed
/// attempt is always a deliberate answer: a driver failure is never rendered
/// as a credential verdict, and a credential verdict never borrows the outage
/// copy. Neither variant carries driver text.
#[derive(Debug, Clone, Copy)]
enum LoginError {
    /// Wrong password, unknown account, empty fields, or valid credentials
    /// without panel access: one 403 with one message (ADR-0013).
    Credentials,
    /// The database behind sign-in could not answer: a 503 that says so,
    /// because the user must not read an outage as a rejected password.
    Unavailable,
}

impl LoginError {
    /// The message the login page's alert carries.
    fn message(self) -> &'static str {
        match self {
            Self::Credentials => GENERIC_ERROR,
            Self::Unavailable => UNAVAILABLE_ERROR,
        }
    }

    /// The status a failed attempt answers with.
    fn status(self) -> http::StatusCode {
        match self {
            Self::Credentials => http::StatusCode::FORBIDDEN,
            Self::Unavailable => http::StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

/// The status and route a login attempt renders: a [`LoginError`] page or a
/// redirect back to `next`.
///
/// The login page is a settled view, so [`ViewExt::single`] resolves it into
/// an owned handle before the response is built — no borrowed view escapes.
async fn login_response(
    cx: &Cx,
    error: Option<LoginError>,
    next: String,
) -> topcoat::Result<topcoat::router::response::Response> {
    let page = render_login_page(cx, error, next).await?;
    page.single().await?.into_response(cx)
}

/// `GET {prefix}/login` — the standalone login page.
pub(crate) fn login_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    let next = next_from_query(cx).unwrap_or_default();
    Box::pin(login_response(cx, None, next))
}

/// `POST {prefix}/login` — verify, rotate the session, redirect to `next`.
pub(crate) fn login_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        // Login/logout carry no file parts: the values half is all they read.
        let values = crate::panel::parse_form_body(cx, body).await?.values;
        crate::csrf::verify(cx, &values)?;
        // Keep a validated destination across a failed attempt so the retry
        // form still returns where the visitor was headed (US6).
        let next = values
            .get(NEXT_FIELD)
            .and_then(|value| safe_next(value))
            .map(str::to_string)
            .or_else(|| next_from_query(cx))
            .unwrap_or_default();
        let email = values.get(LOGIN_FIELD).map(|value| value.trim());
        let password = values.get(PASSWORD_FIELD).map(String::as_str);
        let panel = current(cx).ok_or_else(topcoat::router::error::not_found)?;
        let verified = match (panel.auth.authenticator(), email, password) {
            (Some(authenticator), Some(email), Some(password))
                if !email.is_empty() && !password.is_empty() =>
            {
                match authenticator.verify(cx, email, password).await {
                    Ok(user) => user,
                    // A driver failure is not a credential verdict: the page
                    // says sign-in is unavailable instead of rendering a
                    // rejection. The seam logs the driver's text and
                    // hands back an app-authored error untouched.
                    Err(error) => {
                        let error = infrastructure_failure(error);
                        if crate::error::TabloError::is_infrastructure(&error) {
                            return login_response(cx, Some(LoginError::Unavailable), next).await;
                        }
                        return Err(error);
                    }
                }
            }
            _ => None,
        };
        // One path for every failure: wrong password, unknown account, empty
        // fields, or valid credentials without panel access (ADR-0013).
        let Some(user) = verified.filter(|user| user.can_access_panel) else {
            return login_response(cx, Some(LoginError::Credentials), next).await;
        };
        // Rotate on login: a token this request presented cannot be replayed.
        if let Some(hash) = session::token_hash(cx).await? {
            delete_session(cx, &hash).await?;
        }
        // Bounded housekeeping: the sweep goes with the rotation. A failure is
        // logged rather than fatal — a credential that verified must not become
        // a 503 because cleanup could not run.
        if let Err(error) = sweep_expired_sessions(cx).await {
            tracing::error!(error = %error, "expired-session sweep failed");
        }
        let session = session::start(cx).await?;
        let mut db = crate::db::db(cx);
        let recorded = toasty::create!(AuthSession {
            token_hash: token_key(&session.token_hash),
            user_id: user.id.clone(),
            panel: panel.prefix.clone(),
            expires_at: Timestamp::try_from(session.expires_at).map_err(topcoat::Error::from)?,
            created_at: Timestamp::now(),
        })
        .exec(&mut db)
        .await;
        if let Err(error) = recorded {
            // The credentials were right; the session row could not be
            // recorded. Same outage page as a failed verification — never the
            // driver's text.
            let error = infrastructure_failure(error);
            if crate::error::TabloError::is_infrastructure(&error) {
                return login_response(cx, Some(LoginError::Unavailable), next).await;
            }
            return Err(error);
        }
        let target = if next.is_empty() {
            panel_root(cx)
        } else {
            next
        };
        // Success stays on the `Ok` path so `Set-Cookie` flushes
        // (upstream topcoat#126).
        topcoat::router::error::see_other(target).into_response(cx)
    })
}

/// `POST {prefix}/logout` — delete the session row and clear the cookie.
pub(crate) fn logout_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        // Defense in depth: the route only exists on gated panels, but it
        // re-checks so a missing layer cannot leave logout ungated. Any
        // resolved identity may log out — the gate answers this route for a
        // `can_access_panel=false` user too, so demanding panel
        // access here would strand their session row + cookie to expiry.
        if current_user(cx).is_none() {
            return Err(unauthenticated_error(cx));
        }
        // Login/logout carry no file parts: the values half is all they read.
        let values = crate::panel::parse_form_body(cx, body).await?.values;
        crate::csrf::verify(cx, &values)?;
        if let Some(hash) = session::stop(cx).await? {
            delete_session(cx, &hash).await?;
        }
        let target = login_url(cx);
        topcoat::router::error::see_other(target).into_response(cx)
    })
}

/// The standalone login document: brand and dark mode honored, CSRF hidden
/// field, one error slot carrying a [`LoginError`]'s deliberate copy, no
/// sidebar (ADR-0013).
async fn render_login_page<'a>(
    cx: &'a Cx,
    error: Option<LoginError>,
    next: String,
) -> topcoat::Result<BoxView<'a>> {
    let csrf = crate::csrf::ensure_token(cx);
    let action = login_url(cx);
    let brand = Panel::render_brand(cx).await?;
    let hint = current(cx).and_then(|panel| panel.login_hint.clone());
    let body = topcoat::view::view! {
        cx =>
        <div class="flex min-h-svh items-center justify-center bg-muted p-6">
            <div
                class="flex w-full max-w-sm flex-col gap-6 rounded-xl border border-border bg-card p-6 text-card-foreground shadow-sm"
            >
                if let Some(error) = error {
                    (error.status())
                }
                <div class="flex flex-col items-center gap-2">
                    (brand)
                    <h1 class="text-lg font-semibold text-foreground">"Sign in"</h1>
                </div>
                <form method="post" action=(action) class="flex flex-col gap-4">
                    (crate::csrf::field(cx, &csrf))
                    <input type="hidden" name=(NEXT_FIELD) value=(next)>
                    if let Some(error) = error {
                        tablo_ui::alert(
                            variant: tablo_ui::AlertVariant::Destructive,
                            attrs: topcoat::view::attributes! { role="alert" },
                            tablo_ui::alert_title((error.message()))
                        )
                    }
                    tablo_ui::field(
                        tablo_ui::field_label(
                            attrs: topcoat::view::attributes! { for="email" },
                            "Email or username"
                        )
                        tablo_ui::input(
                            attrs: topcoat::view::attributes! {
                                id="email"
                                name=(LOGIN_FIELD)
                                type="text"
                                required=""
                                autocomplete="username"
                                autofocus=""
                            }
                        )
                    )
                    tablo_ui::field(
                        tablo_ui::field_label(
                            attrs: topcoat::view::attributes! { for="password" },
                            "Password"
                        )
                        tablo_ui::input(
                            attrs: topcoat::view::attributes! {
                                id="password"
                                name=(PASSWORD_FIELD)
                                type="password"
                                required=""
                                autocomplete="current-password"
                            }
                        )
                    )
                    tablo_ui::button(
                        variant: tablo_ui::ButtonVariant::Primary,
                        attrs: topcoat::view::attributes! { type="submit" class="w-full" },
                        "Sign in"
                    )
                </form>
                if let Some(hint) = hint {
                    <p class="text-center text-xs text-muted-foreground">(hint)</p>
                }
            </div>
        </div>
    }
    .boxed();
    Panel::render_document(cx, "Sign in".to_string(), body).await
}

/// The runtime prefix whose unauthenticated requests answer 401 instead of a
/// redirect.
pub(crate) const RUNTIME_PREFIX: &str = "/_topcoat/runtime";

/// The runtime-header value marking a page re-run POST (`true`, per Topcoat's
/// page-rerun protocol).
static RERUN_MARKER: http::HeaderValue = http::HeaderValue::from_static("true");

/// Fail loudly at startup when a required shipped model is missing from the
/// app's `Db` (ADR-0013): the table is never pushed, and the first login
/// would otherwise be a confusing runtime error.
pub(crate) fn assert_models_registered(db: &Db, auth: &Auth) {
    if auth.is_disabled() {
        return;
    }
    let registered = |name: &str| {
        db.schema()
            .app
            .models()
            .any(|model| model.name().upper_camel_case() == name)
    };
    let missing: Vec<&str> = [
        (!registered("AuthSession")).then_some("AuthSession"),
        (matches!(auth, Auth::Password(_)) && !registered("AdminUser")).then_some("AdminUser"),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert!(
        missing.is_empty(),
        "tablo auth is on by default but its shipped models are not registered on the Db \
         (missing {}). Register them with `toasty::models!(…, tablo_core::auth::AdminUser, \
         tablo_core::auth::AuthSession)`, or opt out with `.auth(Auth::disabled())`.",
        missing.join(", "),
    );
}

#[cfg(test)]
mod tests;
