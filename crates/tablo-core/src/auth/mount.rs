//! What a panel mounts for its auth: the declaration checks the mount runs, and every route auth
//! serves under the panel's prefix with the body cap each one carries.

use topcoat::router::{BodyLimit, PageFn, RouteFn, RouterBuilder};

use super::{
    Auth,
    login::{MAX_LOGIN_BYTES, login_page, login_post, logout_post, tenant_post},
    profile::{profile_page, profile_post},
    registration::{register_page, register_post},
};
use crate::{DeclarationErrorKind, panel::route_path, toasty_compat::model::AppSchema};

/// Every mistake in `auth` the mount refuses: a shipped model missing from `db`, and the
/// registration, profile and password-reset declarations. `toggles` says whether some list
/// declares a toggleable column, and `has_mail` whether the router can send mail.
pub(crate) fn declaration_errors(
    db: &toasty::Db,
    auth: &Auth,
    toggles: bool,
    has_mail: bool,
) -> Vec<DeclarationErrorKind> {
    missing_models(db, auth, toggles)
        .err()
        .into_iter()
        .chain(super::registration::declaration_errors(auth))
        .chain(super::profile::declaration_errors(auth))
        .chain(super::password_reset::declaration_errors(auth, has_mail))
        .collect()
}

/// Fail loudly at mount when a required shipped model is missing from the
/// app's `Db` (ADR-0013): the table is never pushed, and the first login
/// would otherwise be a confusing runtime error. [`ColumnPreference`](super::ColumnPreference) is
/// required only when some list declares a toggleable column (`toggles`).
fn missing_models(db: &toasty::Db, auth: &Auth, toggles: bool) -> Result<(), DeclarationErrorKind> {
    let Some(authenticator) = auth.authenticator() else {
        return Ok(());
    };
    let schema = AppSchema::of_db(db);
    let registered = |name: &str| schema.registers(name);
    let shipped_user = loads_admin_users(authenticator);
    let models: Vec<&'static str> = [
        (!registered("AuthSession")).then_some("AuthSession"),
        (shipped_user && !registered("AdminUser")).then_some("AdminUser"),
        (toggles && !registered("ColumnPreference")).then_some("ColumnPreference"),
        (auth.reset.is_some() && !registered("PasswordResetToken")).then_some("PasswordResetToken"),
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

/// Whether `authenticator` loads the shipped [`AdminUser`](super::AdminUser).
#[cfg(feature = "password")]
fn loads_admin_users(authenticator: &dyn super::DynAuthenticator) -> bool {
    authenticator.user_type() == std::any::TypeId::of::<super::AdminUser>()
}

/// Without the `password` feature no authenticator loads a shipped user.
#[cfg(not(feature = "password"))]
fn loads_admin_users(_authenticator: &dyn super::DynAuthenticator) -> bool {
    false
}

/// Mount `auth`'s routes under `prefix`: sign-in, sign-out and the tenant switch while the panel
/// gates, then the sign-up, profile and password-reset pages `auth` declares. Each form route
/// carries [`MAX_LOGIN_BYTES`] in place of the panel's form cap.
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
    super::password_reset::routes(builder, prefix, auth)
}
