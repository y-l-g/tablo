//! The password reset without the `mail` feature: [`Auth`](super::Auth) cannot hold one, so no
//! panel offers it, and the gate, the login page and the session revocations find nothing to do.

use topcoat::{context::Cx, router::RouterBuilder};

/// Uninhabited: without `mail` there is no `Auth::password_reset` to install one.
#[derive(Debug)]
pub(crate) enum PasswordReset {}

/// No panel offers a reset, so the login page links to none.
pub(super) fn forgot_link(_cx: &Cx) -> Option<String> {
    None
}

/// No panel serves a reset page.
pub(super) fn is_reset_page(_cx: &Cx, _path: &str) -> bool {
    false
}

/// Nothing mails, so whether the router could does not matter.
pub(crate) fn router_sends_mail(_builder: &RouterBuilder) -> bool {
    false
}

/// No reset pages to mount.
pub(super) fn routes(builder: RouterBuilder, _prefix: &str, _auth: &super::Auth) -> RouterBuilder {
    builder
}

/// No reset links exist to end.
pub(super) async fn end_links(
    _ex: &mut dyn toasty::Executor,
    _user_id: &str,
    _prefixes: Option<Vec<String>>,
) -> topcoat::Result<()> {
    Ok(())
}

/// No reset to declare wrongly.
pub(super) fn declaration_errors(
    _auth: &super::Auth,
    _has_mail: bool,
) -> Vec<crate::DeclarationErrorKind> {
    Vec::new()
}
