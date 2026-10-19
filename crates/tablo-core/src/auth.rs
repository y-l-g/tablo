//! Authenticates panel users and gates panel routes.
//!
//! Gates panels by default with [`PasswordAuth`] or a custom [`Authenticator`], reads the signed-in
//! user through [`user`] and [`require_user`], and keeps each panel's [`AuthSession`]s.

mod columns;
mod config;
mod gate;
mod login;
mod models;
pub(crate) mod mount;
mod page;
#[cfg(feature = "password")]
mod password;
// Without `mail`, a stand-in that no panel offers keeps the gate and sessions free of `cfg`.
#[cfg_attr(not(feature = "mail"), path = "auth/password_reset_off.rs")]
mod password_reset;
mod profile;
mod registration;
mod session;
mod throttle;

use std::{
    any::{Any, TypeId},
    future::Future,
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

#[cfg(feature = "password")]
pub use self::password::{AdminUser, PasswordAuth, create_admin, hash_password, verify_password};
#[cfg(feature = "mail")]
pub use self::password_reset::{
    PasswordReset, PasswordResetToken, PasswordResetter, RESET_LIFETIME,
};
pub use self::{
    columns::ColumnPreference,
    config::Auth,
    login::{LOGIN_FIELD, NEXT_FIELD, PASSWORD_FIELD, TENANT_FIELD},
    models::register_models,
    profile::{Profile, ProfileEditor, ProfileUpdate},
    registration::{MIN_PASSWORD_CHARS, Registrar, SignUp, new_password_errors},
    session::{AuthSession, SESSION_LIFETIME, mint_session, revoke_sessions_for_user},
    throttle::LoginThrottle,
};
pub(crate) use self::{
    columns::{load_choice, save_choice},
    config::or_default,
    gate::{PanelGate, RuntimeGate},
    login::{logout_url, safe_next, tenant_url},
    password_reset::router_sends_mail,
    profile::{offered as profile_offered, profile_url},
};
use crate::{
    BoxFuture,
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

    /// The name the user menu shows.
    fn display_name(&self) -> &str;

    /// The muted line under the name in the user menu, such as an email or a role. Defaults to
    /// none.
    fn subtitle(&self) -> Option<&str> {
        None
    }

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

/// [`Authenticator`] with its user type erased, as the panel keeps it.
pub(crate) trait DynAuthenticator: Send + Sync {
    fn verify<'a>(
        &'a self,
        cx: &'a Cx,
        login: &'a str,
        password: &'a str,
    ) -> BoxFuture<'a, topcoat::Result<Option<Arc<dyn PanelUser>>>>;
    fn find_by_id<'a>(
        &'a self,
        cx: &'a Cx,
        id: &'a str,
    ) -> BoxFuture<'a, topcoat::Result<Option<Arc<dyn PanelUser>>>>;
    /// The [`TypeId`] of [`Authenticator::User`], so mounting knows when the
    /// shipped [`AdminUser`] must be registered.
    fn user_type(&self) -> TypeId;
}

impl<A: Authenticator> DynAuthenticator for A {
    fn verify<'a>(
        &'a self,
        cx: &'a Cx,
        login: &'a str,
        password: &'a str,
    ) -> BoxFuture<'a, topcoat::Result<Option<Arc<dyn PanelUser>>>> {
        Box::pin(async move {
            let user = Authenticator::verify(self, cx, login, password).await?;
            Ok(user.map(|user| Arc::new(user) as Arc<dyn PanelUser>))
        })
    }

    fn find_by_id<'a>(
        &'a self,
        cx: &'a Cx,
        id: &'a str,
    ) -> BoxFuture<'a, topcoat::Result<Option<Arc<dyn PanelUser>>>> {
        Box::pin(async move {
            let user = Authenticator::find_by_id(self, cx, id).await?;
            Ok(user.map(|user| Arc::new(user) as Arc<dyn PanelUser>))
        })
    }

    fn user_type(&self) -> TypeId {
        TypeId::of::<A::User>()
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
/// where the page's checks do not run. A form route also verifies its CSRF token with
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
    if page_load(cx) {
        redirect(login::login_url_with_next(cx)).into()
    } else {
        unauthorized().into()
    }
}

/// Whether the request loads a page: a `GET` or `HEAD` outside Topcoat's runtime path that is not
/// a page re-run.
pub(crate) fn page_load(cx: &Cx) -> bool {
    let page_method = matches!(*method(cx), http::Method::GET | http::Method::HEAD);
    // A page re-run reaches the gate as a rewritten GET: Topcoat's runtime
    // layer rewrites the browser's marked POST into a GET for the page's own
    // URL. The original request tells it apart from a plain page load.
    let rerun = !matches!(*original_method(cx), http::Method::GET | http::Method::HEAD)
        && original_headers(cx).get(&topcoat::runtime::RUNTIME_HEADER) == Some(&RERUN_MARKER);
    page_method
        && !rerun
        && !uri(cx)
            .path()
            .starts_with(crate::topcoat_compat::RUNTIME_PREFIX)
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

#[cfg(test)]
mod tests;
