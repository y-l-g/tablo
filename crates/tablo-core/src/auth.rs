//! Authenticates panel users and gates panel routes.
//!
//! Gates panels by default with [`PasswordAuth`] or a custom
//! [`Authenticator`], reads the signed-in
//! user through [`user`] and [`require_user`], and keeps each panel's [`AuthSession`]s.

mod columns;
pub(crate) mod config;
mod email_change;
mod gate;
mod input;
mod login;
pub(crate) mod mail;
pub(crate) mod mount;
mod page;
pub(crate) mod panel_user;
pub(crate) mod password;
pub(crate) mod password_reset;
pub(crate) mod profile;
mod realm;
pub(crate) mod registration;
mod session;
mod tenant_profile;
pub(crate) mod tenant_registration;
pub(crate) mod throttle;
mod token;
pub(crate) mod two_factor;
pub(crate) mod verification;

use std::{
    any::{Any, TypeId},
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

use self::realm::realms;
pub use self::{
    columns::ColumnPreference,
    email_change::{EmailChange, request_email_change},
    login::{LOGIN_FIELD, NEXT_FIELD, PASSWORD_FIELD, TENANT_FIELD},
    password::{AdminUser, create_admin, hash_password, verify_password},
    password_reset::{PasswordResetter, RESET_LIFETIME},
    registration::{MIN_PASSWORD_CHARS, new_password_errors},
    session::{AuthSession, SESSION_LIFETIME, mint_session, revoke_sessions_for_user},
    tenant_profile::TenantEditor,
    tenant_registration::TenantRegistrar,
    token::{AuthToken, TokenPurpose},
    two_factor::{AuthFactor, FactorKind},
    verification::{EmailVerifier, VERIFICATION_LIFETIME},
};
// What the crate root exports, under the paths this module's own code names.
pub(crate) use self::{
    columns::{load_choice, save_choice},
    config::{Auth, AuthConfig},
    gate::{PanelGate, RuntimeGate, still_signed_in},
    login::{logout_url, safe_next, tenant_url},
    mail::AuthMail,
    panel_user::{Authenticator, PanelUser},
    password::{PasswordAuth, end_changed_credentials},
    password_reset::PasswordReset,
    profile::{Profile, ProfileEditor, ProfileUpdate, offered as profile_offered, profile_url},
    realm::{AuthHost, CurrentRealm, Host, Realm, SettingsPage, current_realm, panel_prefix},
    registration::{Registrar, SignUp},
    tenant_profile::{offered as tenant_profile_offered, tenant_profile_url},
    tenant_registration::{
        TenantRegistration, offered as tenant_registration_offered, register_tenant_url,
    },
    throttle::LoginThrottle,
    two_factor::{TwoFactor, offered as two_factor_offered, settings_url as two_factor_url},
    verification::EmailVerification,
};
use crate::{BoxFuture, tenancy::Membership};

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
    /// Whether `password` is `user`'s; a user of another type has none.
    fn confirm_password<'a>(
        &'a self,
        cx: &'a Cx,
        user: &'a dyn PanelUser,
        password: &'a str,
    ) -> BoxFuture<'a, topcoat::Result<bool>>;
    /// The [`TypeId`] of [`Authenticator::User`], so mounting knows when the
    /// shipped [`AdminUser`] must be registered.
    fn user_type(&self) -> TypeId;
    /// The name of [`Authenticator::User`], for a message that names it.
    fn user_type_name(&self) -> &'static str;
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

    fn confirm_password<'a>(
        &'a self,
        cx: &'a Cx,
        user: &'a dyn PanelUser,
        password: &'a str,
    ) -> BoxFuture<'a, topcoat::Result<bool>> {
        Box::pin(async move {
            let user: &dyn Any = user;
            match user.downcast_ref::<A::User>() {
                Some(user) => Authenticator::confirm_password(self, cx, user, password).await,
                None => Ok(false),
            }
        })
    }

    fn user_type(&self) -> TypeId {
        TypeId::of::<A::User>()
    }

    fn user_type_name(&self) -> &'static str {
        std::any::type_name::<A::User>()
    }
}

/// The user a panel's gate resolved, as request `Cx` carries it.
#[derive(Clone)]
pub(crate) struct SignedIn {
    pub(crate) user: Arc<dyn PanelUser>,
    /// The realm whose session signed the user in: the only panel the user
    /// is the user of.
    pub(crate) realm: Arc<Realm>,
    /// The tenant the session selected, if any. [`tenant_id`](crate::tenant_id)
    /// honors it only while it is one of the user's memberships.
    pub(crate) tenant: Option<Uuid>,
}

impl SignedIn {
    /// `cx` with the user signed in, its words in the user's [`locale`](PanelUser::locale) when
    /// the panel offers a match.
    pub(crate) fn sign_into(self, cx: &Cx) -> Cx {
        crate::i18n::addressed_to(cx, self.user.locale().as_ref()).with(self)
    }
}

/// The request's signed-in user on the request's panel, whatever their panel
/// access. Only the logout route reads a user without access.
pub(crate) fn resolved(cx: &Cx) -> Option<&SignedIn> {
    let signed = try_request_context::<SignedIn>(cx)?;
    match current_realm(cx) {
        Some(realm) if !Arc::ptr_eq(&signed.realm, realm) => None,
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
/// [`tenants`](PanelUser::tenants), else the user's
/// [`default_tenant`](PanelUser::default_tenant), else the first. `None` without a
/// signed-in user, for a user with no tenant, and when a `Tenant` override
/// names a tenant the user is not a member of.
pub fn membership(cx: &Cx) -> Option<&Membership> {
    let tenant = crate::tenant_id(cx)?;
    signed(cx)?
        .user
        .tenants()
        .iter()
        .find(|m| m.tenant.get() == tenant)
}

/// The tenant of the session's selected membership, else of the user's default, else of their
/// first: the [`TenantSource`](crate::tenancy::TenantSource) the panel installs.
pub(crate) fn session_tenant(cx: &Cx) -> Option<Uuid> {
    let signed = signed(cx)?;
    let tenants = signed.user.tenants();
    let member = |tenant: Uuid| tenants.iter().find(|m| m.tenant.get() == tenant);
    signed
        .tenant
        .and_then(member)
        .or_else(|| {
            signed
                .user
                .default_tenant()
                .and_then(|tenant| member(tenant.get()))
        })
        .or_else(|| tenants.first())
        .map(|membership| membership.tenant.get())
}

/// The signed-in user's [`time_zone`](PanelUser::time_zone), else the request panel's
/// [`time_zone`](crate::Panel::time_zone): the [`ZoneSource`](crate::values::ZoneSource) the panel
/// installs.
pub(crate) fn session_time_zone(cx: &Cx) -> Option<jiff::tz::TimeZone> {
    signed(cx)
        .and_then(|signed| signed.user.time_zone())
        .or_else(|| current_realm(cx).map(|realm| realm.time_zone.clone()))
}

/// The request panel's catalog: the [`CatalogSource`](crate::i18n::CatalogSource) the panel
/// installs.
pub(crate) fn session_catalog(cx: &Cx) -> Option<&crate::i18n::Catalog> {
    current_realm(cx).map(|realm| &realm.catalog)
}

/// The signed-in user, as the app's own user type.
///
/// `None` when nobody is signed in to the request's panel, and when the
/// panel's [`Authenticator`] loads another type than `U` — a helper shared by
/// two panels with different user types answers `None` on the other one.
///
/// # Panics
///
/// In a debug build, when no mounted panel's authenticator loads `U`: that `U` can only answer
/// `None`, so every check through it would deny. The message names `U` and the types the panels
/// load.
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
    if cfg!(debug_assertions) {
        assert_some_panel_loads::<U>(cx);
    }
    let user: &dyn Any = &*signed(cx)?.user;
    user.downcast_ref()
}

/// Panics when panels sign users in and none of them loads a `U`.
fn assert_some_panel_loads<U: PanelUser>(cx: &Cx) {
    let mut mounted = realms(cx);
    if mounted.is_empty() {
        mounted.extend(current_realm(cx).cloned());
    }
    let loaded: Vec<&dyn DynAuthenticator> = mounted
        .iter()
        .filter_map(|realm| realm.auth.authenticator())
        .collect();
    if loaded.is_empty()
        || loaded
            .iter()
            .any(|auth| auth.user_type() == TypeId::of::<U>())
    {
        return;
    }
    let names: std::collections::BTreeSet<_> =
        loaded.iter().map(|auth| auth.user_type_name()).collect();
    panic!(
        "auth::user::<{}> can never find a user: the mounted panels sign in {}",
        std::any::type_name::<U>(),
        names.into_iter().collect::<Vec<_>>().join(", "),
    );
}

/// Require the signed-in user, as the app's own user type.
///
/// # Errors
///
/// Answers per request kind (ADR-0013): pages redirect to the login route with
/// a same-origin-relative `next`; runtime endpoints, non-GET requests, and
/// page re-runs (marked POSTs the runtime layer rewrites into GETs) answer
/// 401. A signed-in user of another type than `U` answers 403.
///
/// # Panics
///
/// As [`user`] does, in a debug build, when no mounted panel loads `U`.
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
/// Outside any panel — an app's own page, or a shard no panel declared — it
/// answers whether some mounted panel does, so a check there fails closed.
pub fn enforced(cx: &Cx) -> bool {
    match current_realm(cx) {
        Some(realm) => realm.gates(),
        None => realms(cx).iter().any(|realm| realm.gates()),
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
