//! What a panel mounts for its auth: the declaration checks the mount runs, and every route auth
//! serves under the panel's prefix with the body cap each one carries.

use std::any::TypeId;

use topcoat::router::{BodyLimit, PageFn, RouteFn, RouteHandlerFn, RouterBuilder};

use super::{
    AdminUser, Auth, email_change,
    login::{MAX_LOGIN_BYTES, login_page, login_post, logout_post, tenant_post},
    password_reset::{forgot_page, forgot_post, reset_page, reset_post},
    profile::{profile_page, profile_post},
    registration::{register_page, register_post},
    tenant_profile::{tenant_profile_page, tenant_profile_post},
    tenant_registration::{register_tenant_page, register_tenant_post},
    two_factor::{challenge_page, challenge_post, settings_page, settings_post},
    verification::{confirm_page, notice_page, notice_post},
};
use crate::{DeclarationErrorKind, panel::route_path, toasty_compat::model::AppSchema};

/// What the router installs that auth features need.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Services {
    /// A `MailConfig`, which mailed links go out through.
    pub(crate) mail: bool,
    /// A cookie `Key`, which seals two-factor secrets.
    pub(crate) cookie_key: bool,
}

/// Every mistake in `auth` the mount refuses: a shipped model missing from `db`, and the
/// sign-up, profile, password-reset, email-verification, tenant, two-factor and mail
/// declarations. `toggles` says whether some list declares a toggleable column, `notifications`
/// whether the panel shows the bell, and `services` what the router installs.
pub(crate) fn declaration_errors(
    db: &toasty::Db,
    auth: &Auth,
    toggles: bool,
    notifications: bool,
    services: Services,
) -> Vec<DeclarationErrorKind> {
    if notifications && auth.is_disabled() {
        return vec![DeclarationErrorKind::NotificationsWithoutAuth];
    }
    missing_models(db, auth, toggles, notifications)
        .err()
        .into_iter()
        .chain(super::registration::declaration_errors(auth))
        .chain(super::profile::declaration_errors(auth))
        .chain(super::password_reset::declaration_errors(auth))
        .chain(super::verification::declaration_errors(auth))
        .chain(super::tenant_registration::declaration_errors(auth))
        .chain(super::tenant_profile::declaration_errors(auth))
        .chain(super::two_factor::declaration_errors(
            auth,
            services.cookie_key,
        ))
        .chain(super::mail::declaration_errors(auth, services.mail))
        .collect()
}

/// Fail loudly at mount when a required shipped model is missing from the
/// app's `Db` (ADR-0013): the table is never pushed, and the first login
/// would otherwise be a confusing runtime error. [`ColumnPreference`](super::ColumnPreference) is
/// required only when some list declares a toggleable column (`toggles`), and
/// [`StoredNotification`](crate::notification::StoredNotification) only when the panel shows the
/// bell (`notifications`).
fn missing_models(
    db: &toasty::Db,
    auth: &Auth,
    toggles: bool,
    notifications: bool,
) -> Result<(), DeclarationErrorKind> {
    let Some(authenticator) = auth.authenticator() else {
        return Ok(());
    };
    let schema = AppSchema::of_db(db);
    let registered = |name: &str| schema.registers(name);
    let shipped_user = authenticator.user_type() == TypeId::of::<AdminUser>();
    let models: Vec<&'static str> = [
        (!registered("AuthSession")).then_some("AuthSession"),
        (shipped_user && !registered("AdminUser")).then_some("AdminUser"),
        (toggles && !registered("ColumnPreference")).then_some("ColumnPreference"),
        (auth.issues_tokens() && !registered("AuthToken")).then_some("AuthToken"),
        (auth.two_factor.is_some() && !registered("AuthFactor")).then_some("AuthFactor"),
        (notifications && !registered("StoredNotification")).then_some("StoredNotification"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if models.is_empty() {
        Ok(())
    } else {
        Err(DeclarationErrorKind::MissingAuthModels { models })
    }
}

/// Mount `auth`'s routes under `prefix`: sign-in, sign-out and the tenant switch while the panel
/// gates, then the sign-up, tenant registration, profile, tenant settings, two-factor,
/// password-reset and email-verification pages `auth` declares.
/// Each form route carries [`MAX_LOGIN_BYTES`] in place of the panel's form cap.
pub(crate) fn routes(mut builder: RouterBuilder, prefix: &str, auth: &Auth) -> RouterBuilder {
    if !auth.is_disabled() {
        let login_path = route_path(&format!("{prefix}/login"));
        builder = builder
            .layer(BodyLimit::max(MAX_LOGIN_BYTES).at(login_path.clone()))
            .route(RouteFn::new(
                http::Method::GET,
                login_path.clone(),
                login_page,
            ))
            .route(RouteFn::new(http::Method::POST, login_path, login_post))
            .route(RouteFn::new(
                http::Method::POST,
                route_path(&format!("{prefix}/logout")),
                logout_post,
            ))
            .route(RouteFn::new(
                http::Method::POST,
                route_path(&format!("{prefix}/tenant")),
                tenant_post,
            ));
    }
    if auth.registrar().is_some() {
        let register_path = route_path(&format!("{prefix}/register"));
        builder = builder
            .layer(BodyLimit::max(MAX_LOGIN_BYTES).at(register_path.clone()))
            .route(RouteFn::new(
                http::Method::GET,
                register_path.clone(),
                register_page,
            ))
            .route(RouteFn::new(
                http::Method::POST,
                register_path,
                register_post,
            ));
    }
    if auth.tenant_registration_page().is_some() {
        let path = route_path(&format!("{prefix}/register-tenant"));
        builder = builder
            .layer(BodyLimit::max(MAX_LOGIN_BYTES).at(path.clone()))
            .route(RouteFn::new(
                http::Method::GET,
                path.clone(),
                register_tenant_page,
            ))
            .route(RouteFn::new(http::Method::POST, path, register_tenant_post));
    }
    if auth.profile_editor().is_some() {
        // The profile page renders in the panel's shell, under the panel's own form cap.
        let profile_path = route_path(&format!("{prefix}/profile"));
        builder = builder
            .page(PageFn::new(
                http::Method::GET,
                profile_path.clone(),
                profile_page,
            ))
            .page(PageFn::new(http::Method::POST, profile_path, profile_post));
    }
    if auth.tenant_editor().is_some() {
        // The settings page renders in the panel's shell, under the panel's own form cap.
        let path = route_path(&format!("{prefix}/tenant/profile"));
        builder = builder
            .page(PageFn::new(
                http::Method::GET,
                path.clone(),
                tenant_profile_page,
            ))
            .page(PageFn::new(http::Method::POST, path, tenant_profile_post));
    }
    if auth.two_factor_config().is_some() {
        let challenge = route_path(&format!("{prefix}/two-factor-challenge"));
        // The settings page renders in the panel's shell, under the panel's own form cap.
        let settings = route_path(&format!("{prefix}/two-factor"));
        builder = builder
            .layer(BodyLimit::max(MAX_LOGIN_BYTES).at(challenge.clone()))
            .route(RouteFn::new(
                http::Method::GET,
                challenge.clone(),
                challenge_page,
            ))
            .route(RouteFn::new(http::Method::POST, challenge, challenge_post))
            .page(PageFn::new(
                http::Method::GET,
                settings.clone(),
                settings_page,
            ))
            .page(PageFn::new(http::Method::POST, settings, settings_post));
    }
    if auth.reset().is_some() {
        for (path, page, post) in [
            (
                "forgot-password",
                forgot_page as RouteHandlerFn,
                forgot_post as RouteHandlerFn,
            ),
            ("reset-password", reset_page, reset_post),
        ] {
            let path = route_path(&format!("{prefix}/{path}"));
            builder = builder
                .layer(BodyLimit::max(MAX_LOGIN_BYTES).at(path.clone()))
                .route(RouteFn::new(http::Method::GET, path.clone(), page))
                .route(RouteFn::new(http::Method::POST, path, post));
        }
    }
    if auth.verification().is_some() {
        let notice = route_path(&format!("{prefix}/verify-email"));
        builder = builder
            .layer(BodyLimit::max(MAX_LOGIN_BYTES).at(notice.clone()))
            .route(RouteFn::new(http::Method::GET, notice.clone(), notice_page))
            .route(RouteFn::new(http::Method::POST, notice, notice_post))
            .route(RouteFn::new(
                http::Method::GET,
                route_path(&format!("{prefix}/verify-email/confirm")),
                confirm_page,
            ));
        for (path, page, post) in [
            (
                "email-change/confirm",
                email_change::confirm_page as RouteHandlerFn,
                email_change::confirm_post as RouteHandlerFn,
            ),
            (
                "email-change/cancel",
                email_change::cancel_page,
                email_change::cancel_post,
            ),
        ] {
            let path = route_path(&format!("{prefix}/{path}"));
            builder = builder
                .layer(BodyLimit::max(MAX_LOGIN_BYTES).at(path.clone()))
                .route(RouteFn::new(http::Method::GET, path.clone(), page))
                .route(RouteFn::new(http::Method::POST, path, post));
        }
    }
    builder
}
