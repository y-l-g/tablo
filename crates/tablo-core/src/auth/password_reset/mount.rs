//! What a panel mounts for its password reset: the declaration checks and the two pages' routes.

use topcoat::router::{BodyLimit, RouteFn, RouteHandlerFn, RouterBuilder};

use super::{forgot_page, forgot_post, reset_page, reset_post};
use crate::{auth::login::MAX_LOGIN_BYTES, panel::route_path};

/// Whether `builder` holds the [`MailConfig`](topcoat::mail::MailConfig) a reset mails through.
pub(crate) fn router_sends_mail(builder: &RouterBuilder) -> bool {
    builder
        .get_app_context::<topcoat::mail::MailConfig>()
        .is_some()
}

/// Mount the forgot-password and reset pages under `prefix` when `auth` declares a reset, each
/// form route under [`MAX_LOGIN_BYTES`].
pub(in crate::auth) fn routes(
    mut builder: RouterBuilder,
    prefix: &str,
    auth: &crate::Auth,
) -> RouterBuilder {
    if auth.reset().is_none() {
        return builder;
    }
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
    builder
}

/// The mistakes in a panel's [`Auth::password_reset`](crate::Auth::password_reset): a reset without
/// auth, a resetter whose user is not the authenticator's, a base URL that is not absolute, and a
/// router that cannot send mail.
pub(in crate::auth) fn declaration_errors(
    auth: &crate::Auth,
    has_mail: bool,
) -> Vec<crate::DeclarationErrorKind> {
    use crate::{DeclarationErrorKind, declaration::ResetFault};

    let Some(reset) = auth.reset.as_ref() else {
        return Vec::new();
    };
    let Some(authenticator) = auth.authenticator() else {
        return vec![DeclarationErrorKind::PasswordReset(
            ResetFault::AuthDisabled,
        )];
    };
    let mut faults = Vec::new();
    let (user, user_name) = reset.resetter().user_type();
    if user != authenticator.user_type() {
        faults.push(ResetFault::UserType {
            resetter: user_name,
        });
    }
    if !reset.base_url_is_absolute() {
        faults.push(ResetFault::BaseUrl(reset.base_url().to_string()));
    }
    if !has_mail {
        faults.push(ResetFault::MissingMail);
    }
    faults
        .into_iter()
        .map(DeclarationErrorKind::PasswordReset)
        .collect()
}
