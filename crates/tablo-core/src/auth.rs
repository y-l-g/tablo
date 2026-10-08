//! Authenticates panel users and gates panel routes.
//!
//! Gates panels by default; installs [`PasswordAuth`] or a custom [`Authenticator`].
//! Reads the signed-in user through [`user`] and [`require_user`]. Stores sessions in
//! [`AuthSession`] for every authenticator, scoped to the issuing panel.

mod gate;
mod login;
mod password;
mod session;

use std::{
    any::{Any, TypeId},
    future::Future,
    pin::Pin,
    sync::Arc,
};

use topcoat::{
    context::{Cx, try_request_context},
    router::{
        error::{forbidden, redirect, unauthorized},
        request::{method, original_headers, original_method, uri},
    },
};
use uuid::Uuid;

pub(crate) use self::{
    gate::{PanelGate, RuntimeGate},
    login::{
        MAX_LOGIN_BYTES, login_page, login_post, logout_post, logout_url, safe_next, tenant_post,
        tenant_url,
    },
};
pub use self::{
    login::{LOGIN_FIELD, NEXT_FIELD, PASSWORD_FIELD, TENANT_FIELD},
    password::{AdminUser, PasswordAuth, create_admin, hash_password, verify_password},
    session::{AuthSession, SESSION_LIFETIME, mint_session, revoke_sessions_for_user},
};
use crate::{
    panel::{
        panel_prefix,
        state::{PanelState, Panels, current, panels},
    },
    tenancy::Membership,
};

/// A signed-in user, as the panel knows it (ADR-0013).
///
/// Implement it for the app's own user type and return that type from an
/// [`Authenticator`]; the shipped [`AdminUser`] implements it too. App code
/// reads the user back with its own type through [`user`].
///
/// ```rust
/// # struct Staff {
/// #     id: uuid::Uuid,
/// #     name: String,
/// #     active: bool,
/// #     memberships: Vec<tablo_core::Membership>,
/// # }
/// # use tablo_core::{Membership, PanelUser};
/// impl PanelUser for Staff {
///     fn user_id(&self) -> String {
///         self.id.to_string()
///     }
///     fn display_name(&self) -> &str {
///         &self.name
///     }
///     fn can_access_panel(&self) -> bool {
///         self.active
///     }
///     fn tenants(&self) -> &[Membership] {
///         &self.memberships
///     }
/// }
/// ```
pub trait PanelUser: Any + Send + Sync {
    /// The stable key the session stores and
    /// [`Authenticator::find_by_id`] receives. Any string fits.
    fn user_id(&self) -> String;

    /// The name the panel's top bar shows.
    fn display_name(&self) -> &str;

    /// Whether the user may enter the panel. A user who may not answers 403,
    /// indistinguishable from bad credentials at login. Defaults to `true`.
    fn can_access_panel(&self) -> bool {
        true
    }

    /// The tenants the user may act for, in the order the tenant switcher
    /// lists them. The first is the default tenant until the user selects
    /// another. Defaults to none: the user has no tenant.
    fn tenants(&self) -> &[Membership] {
        &[]
    }
}

/// How a panel loads its users (ADR-0013).
///
/// The default [`PasswordAuth`] implements it against the shipped
/// [`AdminUser`] model; an app with an existing user table implements it and
/// passes the value to [`Panel::auth`](crate::Panel::auth) via
/// [`Auth::custom`]. Sessions are the framework's, so an implementation only
/// maps credentials to a user and a stored id back to one.
///
/// Every *credential* failure must return `Ok(None)`, never a distinguishable
/// error: the login response is one generic message for all of them. An
/// infrastructure failure is not a credential verdict, so an implementation
/// that cannot reach its store returns the driver's error instead — the login
/// handler maps it to the opaque outage page, which keeps a database outage
/// from rendering as a rejected password. An implementation's own error keeps
/// its own mapping.
pub trait Authenticator: Send + Sync + 'static {
    /// The app's user type.
    type User: PanelUser;

    /// Verify `login`/`password`, returning the user on success.
    ///
    /// Implementations must run comparable work for unknown accounts so
    /// timing does not leak account existence; [`verify_password`] does, given
    /// `None` for an unknown account.
    fn verify(
        &self,
        cx: &Cx,
        login: &str,
        password: &str,
    ) -> impl Future<Output = topcoat::Result<Option<Self::User>>> + Send;

    /// Load the session's user by [`PanelUser::user_id`], with everything the request
    /// reads from it — its [`tenants`](PanelUser::tenants) included.
    ///
    /// It runs on every request, which is what makes deactivation, revocation
    /// and a removed membership take effect immediately; return `None` when
    /// the user no longer authenticates.
    fn find_by_id(
        &self,
        cx: &Cx,
        id: &str,
    ) -> impl Future<Output = topcoat::Result<Option<Self::User>>> + Send;
}

/// The boxed future the erased authenticator returns.
type UserFuture<'a> =
    Pin<Box<dyn Future<Output = topcoat::Result<Option<Arc<dyn PanelUser>>>> + Send + 'a>>;

/// [`Authenticator`] with its user type erased, as the panel keeps it.
pub(crate) trait DynAuthenticator: Send + Sync {
    fn verify<'a>(&'a self, cx: &'a Cx, login: &'a str, password: &'a str) -> UserFuture<'a>;
    fn find_by_id<'a>(&'a self, cx: &'a Cx, id: &'a str) -> UserFuture<'a>;
    /// The [`TypeId`] of [`Authenticator::User`], so mounting knows when the
    /// shipped [`AdminUser`] must be registered.
    fn user_type(&self) -> TypeId;
}

impl<A: Authenticator> DynAuthenticator for A {
    fn verify<'a>(&'a self, cx: &'a Cx, login: &'a str, password: &'a str) -> UserFuture<'a> {
        Box::pin(async move {
            let user = Authenticator::verify(self, cx, login, password).await?;
            Ok(user.map(|user| Arc::new(user) as Arc<dyn PanelUser>))
        })
    }

    fn find_by_id<'a>(&'a self, cx: &'a Cx, id: &'a str) -> UserFuture<'a> {
        Box::pin(async move {
            let user = Authenticator::find_by_id(self, cx, id).await?;
            Ok(user.map(|user| Arc::new(user) as Arc<dyn PanelUser>))
        })
    }

    fn user_type(&self) -> TypeId {
        TypeId::of::<A::User>()
    }
}

/// The panel's authentication configuration (ADR-0013).
///
/// The default is [`Auth::password`]; [`Auth::custom`] installs an app-owned
/// [`Authenticator`]; [`Auth::disabled`] is the explicit fail-open opt-out.
pub struct Auth(Option<Box<dyn DynAuthenticator>>);

impl Auth {
    /// The shipped Argon2id + [`AdminUser`] authenticator.
    #[must_use]
    pub fn password() -> Self {
        Self::custom(PasswordAuth)
    }

    /// An app-owned authenticator over its own user table.
    #[must_use]
    pub fn custom(authenticator: impl Authenticator) -> Self {
        Self(Some(Box::new(authenticator)))
    }

    /// Explicit fail-open opt-out for public demos (ADR-0013): no gate, no
    /// login routes, sessions unused.
    #[must_use]
    pub fn disabled() -> Self {
        Self(None)
    }

    /// Whether the explicit opt-out is set.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.0.is_none()
    }

    /// The authenticator, or `None` when auth is disabled.
    pub(crate) fn authenticator(&self) -> Option<&dyn DynAuthenticator> {
        self.0.as_deref()
    }
}

impl Default for Auth {
    fn default() -> Self {
        Self::password()
    }
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.is_disabled() {
            "Auth::disabled"
        } else {
            "Auth"
        })
    }
}

/// The user a panel's gate resolved, as request `Cx` carries it.
#[derive(Clone)]
pub(crate) struct SignedIn {
    pub(crate) user: Arc<dyn PanelUser>,
    /// The panel whose session signed the user in: the only panel the user
    /// is the user of.
    pub(crate) panel: Arc<PanelState>,
    /// The tenant the session selected, if any. [`tenant_id`](crate::tenant_id)
    /// honors it only while it is one of the user's memberships.
    pub(crate) tenant: Option<Uuid>,
}

/// The request's signed-in user on the request's panel, whatever their panel
/// access. Only the logout route reads a user without access.
pub(crate) fn resolved(cx: &Cx) -> Option<&SignedIn> {
    let signed = try_request_context::<SignedIn>(cx)?;
    match current(cx) {
        Some(panel) if !Arc::ptr_eq(&signed.panel, panel) => None,
        _ => Some(signed),
    }
}

/// The request's signed-in user, erased: on the request's panel, with panel
/// access.
pub(crate) fn signed(cx: &Cx) -> Option<&SignedIn> {
    resolved(cx).filter(|signed| signed.user.can_access_panel())
}

/// The signed-in user's membership the request acts under: the tenant the
/// session selected while it is still one of the user's
/// [`tenants`](PanelUser::tenants), else the first. `None` without a
/// signed-in user, for a user with no tenant, and when a `Tenant` override
/// names a tenant the user is not a member of.
pub fn membership(cx: &Cx) -> Option<&Membership> {
    let tenant = crate::tenant_id(cx)?;
    signed(cx)?
        .user
        .tenants()
        .iter()
        .find(|m| m.tenant == tenant)
}

/// The tenant of the session's selected membership, else of the user's first: the
/// [`TenantSource`](crate::tenancy::TenantSource) the panel installs.
pub(crate) fn session_tenant(cx: &Cx) -> Option<Uuid> {
    let signed = signed(cx)?;
    let tenants = signed.user.tenants();
    signed
        .tenant
        .and_then(|tenant| tenants.iter().find(|m| m.tenant == tenant))
        .or_else(|| tenants.first())
        .map(|membership| membership.tenant)
}

/// The signed-in user, as the app's own user type.
///
/// `None` when nobody is signed in to the request's panel, and when the
/// panel's [`Authenticator`] loads another type than `U` — a helper shared by
/// two panels with different user types answers `None` on the other one.
///
/// ```rust
/// # struct Staff { admin: bool }
/// # use tablo_core::{PanelUser, auth};
/// # use topcoat::context::Cx;
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { String::new() }
/// #     fn display_name(&self) -> &str { "" }
/// # }
/// fn admins_only(cx: &Cx) -> bool {
///     auth::user::<Staff>(cx).is_some_and(|staff| staff.admin)
/// }
/// ```
pub fn user<U: PanelUser>(cx: &Cx) -> Option<&U> {
    let user: &dyn Any = &*signed(cx)?.user;
    user.downcast_ref()
}

/// Require the signed-in user, as the app's own user type.
///
/// # Errors
///
/// Answers per request kind (ADR-0013): pages redirect to the login route with
/// a same-origin-relative `next`; runtime endpoints, non-GET requests, and
/// page re-runs (marked POSTs the runtime layer rewrites into GETs) answer
/// 401. A signed-in user of another type than `U` answers 403.
pub fn require_user<U: PanelUser>(cx: &Cx) -> topcoat::Result<&U> {
    match signed(cx) {
        Some(_) => user(cx).ok_or_else(|| forbidden().into()),
        None => Err(unauthenticated_error(cx)),
    }
}

/// Whether a user is signed in to the request's panel.
pub fn signed_in(cx: &Cx) -> bool {
    signed(cx).is_some()
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

/// Require a signed-in user when the request's panel requires sign-in.
///
/// Every panel handler runs it first, and a page, route or shard the app
/// serves under a panel runs it to answer exactly as the panel's own pages do.
/// Shards are the case that needs it: Topcoat serves them at its runtime path,
/// where no page guard runs. A form route also verifies its CSRF token with
/// [`csrf::verify`](crate::csrf::verify).
///
/// # Errors
///
/// 401, or a redirect to the login page, as [`require_user`] answers.
pub fn guard(cx: &Cx) -> topcoat::Result<()> {
    if enforced(cx) && !signed_in(cx) {
        return Err(unauthenticated_error(cx));
    }
    Ok(())
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
    if path.starts_with(crate::topcoat_compat::RUNTIME_PREFIX) || !page_method || rerun {
        unauthorized().into()
    } else {
        redirect(login::login_url_with_next(cx)).into()
    }
}

/// The panel root: where a completed login or tenant switch lands.
fn panel_root(cx: &Cx) -> String {
    panel_prefix(cx)
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

/// What a failed login attempt renders when the database behind it could not
/// answer.
///
/// The credential rejection is deliberately non-specific about *why*
/// credentials were refused, so reusing it for an outage would tell a user
/// their password was wrong while the database was down — the one place they
/// would retry uselessly. This copy names the machinery that is down instead,
/// and like the generic one it never carries driver text: that goes to the
/// log.
const UNAVAILABLE_ERROR: &str = "Sign-in is unavailable right now. Try again shortly.";

/// The runtime-header value marking a page re-run POST (`true`, per Topcoat's
/// page-rerun protocol).
static RERUN_MARKER: http::HeaderValue = http::HeaderValue::from_static("true");

/// Fail loudly at mount when a required shipped model is missing from the
/// app's `Db` (ADR-0013): the table is never pushed, and the first login
/// would otherwise be a confusing runtime error.
pub(crate) fn check_models_registered(
    db: &toasty::Db,
    auth: &Auth,
) -> Result<(), crate::DeclarationErrorKind> {
    let Some(authenticator) = auth.authenticator() else {
        return Ok(());
    };
    let schema = crate::toasty_compat::model::AppSchema::of_db(db);
    let registered = |name: &str| schema.registers(name);
    let shipped_user = authenticator.user_type() == TypeId::of::<AdminUser>();
    let models: Vec<&'static str> = [
        (!registered("AuthSession")).then_some("AuthSession"),
        (shipped_user && !registered("AdminUser")).then_some("AdminUser"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if models.is_empty() {
        Ok(())
    } else {
        Err(crate::DeclarationErrorKind::MissingAuthModels { models })
    }
}

#[cfg(test)]
mod tests;
