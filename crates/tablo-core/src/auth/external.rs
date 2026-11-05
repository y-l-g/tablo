//! [`ExternalAuth`]: a panel whose users sign in through the host app.

use std::sync::Arc;

use tablo_ui::{ButtonSize, ButtonVariant, button_variants};
use topcoat::{
    context::Cx,
    icon::icon,
    router::{
        Body, RouteFuture,
        error::{redirect, see_other},
        response::IntoResponse,
    },
    view::{ViewExt, view},
};

use super::{
    Authenticator, PanelUser,
    gate::refused,
    infrastructure_failure,
    login::{login_url, next_from_query},
    panel_root,
    realm::current_realm,
    registration::with_next,
    session::{fresh_session, record, sweep_expired_sessions},
};
use crate::{BoxFuture, DeclarationErrorKind, auth::AuthConfig};

/// A host app's own sign-in, which a panel trusts in place of a login form (ADR-0013).
///
/// [`Auth::external`](crate::Auth::external) installs it. The app keeps its login page, its
/// sessions and its credentials; a panel user is whoever
/// [`current_user`](Self::current_user) says the request's app session belongs to.
///
/// An anonymous request to the panel's `{prefix}/login` goes to
/// [`login_url`](Self::login_url). Once the app has signed the user in and sent them back, the
/// panel starts a panel session of its own, which holds what the panel keeps per browser, such as
/// the selected tenant. On every later request the gate asks `current_user` again, and a session
/// whose user it no longer names ends: signing out of the app, or into another account, takes
/// effect on the panel's next request. A browser holds one panel session, so on two panels each
/// page load on the other starts its session again.
///
/// ```rust
/// use tablo_core::{ExternalAuth, PanelUser, auth::safe_next};
/// use topcoat::{context::Cx, session};
///
/// struct Staff {
///     id: String,
///     name: String,
///     admin: bool,
/// }
///
/// impl PanelUser for Staff {
///     fn user_id(&self) -> String {
///         self.id.clone()
///     }
///
///     fn display_name(&self) -> &str {
///         &self.name
///     }
///
///     fn can_access_panel(&self, _panel: &str) -> bool {
///         self.admin
///     }
/// }
///
/// async fn staff_for(_cx: &Cx, _hash: &session::TokenHash) -> topcoat::Result<Option<Staff>> {
///     Ok(None) // the app's own session lookup
/// }
///
/// struct AppSession;
///
/// impl ExternalAuth for AppSession {
///     type User = Staff;
///
///     async fn current_user(&self, cx: &Cx) -> topcoat::Result<Option<Staff>> {
///         match session::token_hash(cx).await? {
///             Some(hash) => staff_for(cx, &hash).await,
///             None => Ok(None),
///         }
///     }
///
///     fn login_url(&self, _cx: &Cx, next: &str) -> String {
///         format!("/login?return_to={next}")
///     }
/// }
/// ```
pub trait ExternalAuth: Send + Sync + 'static {
    /// The app's user type.
    type User: PanelUser;

    /// The user the request's app session signs in; `None` when it signs in nobody.
    ///
    /// It runs on the gate of every panel request, and again when a live connection checks its
    /// session, long after the response started: it reads the request and never writes a cookie,
    /// so a sliding app session refreshes elsewhere. A failure to reach the app's store returns
    /// the error, which answers as a sign-in outage rather than as a signed-out user.
    fn current_user(
        &self,
        cx: &Cx,
    ) -> impl Future<Output = topcoat::Result<Option<Self::User>>> + Send;

    /// Where a request with no app session signs in. `next` is the URL to come back to,
    /// percent-encoded to sit in a query string. Check it with
    /// [`safe_next`](crate::auth::safe_next) before redirecting to it: the browser can send
    /// anything.
    fn login_url(&self, cx: &Cx, next: &str) -> String;

    /// Where the shell's Sign out lands, once it has ended the panel's session. It ends only that
    /// one: name a route that also ends the app's, or the user's next panel request signs them
    /// in again. Defaults to `/`.
    fn logout_url(&self, _cx: &Cx) -> String {
        "/".to_owned()
    }
}

/// [`ExternalAuth`] with its user type erased, as the panel keeps it.
pub(crate) trait DynExternal: Send + Sync {
    fn current_user<'a>(
        &'a self,
        cx: &'a Cx,
    ) -> BoxFuture<'a, topcoat::Result<Option<Arc<dyn PanelUser>>>>;
    fn login_url(&self, cx: &Cx, next: &str) -> String;
    fn logout_url(&self, cx: &Cx) -> String;
}

impl<E: ExternalAuth> DynExternal for E {
    fn current_user<'a>(
        &'a self,
        cx: &'a Cx,
    ) -> BoxFuture<'a, topcoat::Result<Option<Arc<dyn PanelUser>>>> {
        Box::pin(async move {
            let user = ExternalAuth::current_user(self, cx).await?;
            Ok(user.map(|user| Arc::new(user) as Arc<dyn PanelUser>))
        })
    }

    fn login_url(&self, cx: &Cx, next: &str) -> String {
        ExternalAuth::login_url(self, cx, next)
    }

    fn logout_url(&self, cx: &Cx) -> String {
        ExternalAuth::logout_url(self, cx)
    }
}

/// The [`Authenticator`] over an [`ExternalAuth`]: no password signs in, and a session loads its
/// user only while the app's session names the same one, so the gates treat it as any other.
pub(super) struct ExternalUsers<E>(pub(super) Arc<E>);

impl<E: ExternalAuth> Authenticator for ExternalUsers<E> {
    type User = E::User;

    async fn verify(
        &self,
        _cx: &Cx,
        _login: &str,
        _password: &str,
    ) -> topcoat::Result<Option<E::User>> {
        Ok(None)
    }

    async fn find_by_id(&self, cx: &Cx, id: &str) -> topcoat::Result<Option<E::User>> {
        let user = ExternalAuth::current_user(&*self.0, cx).await?;
        Ok(user.filter(|user| user.user_id() == id))
    }
}

/// `GET {prefix}/login` on a panel behind [`ExternalAuth`]: starts a panel session for the app's
/// signed-in user and lands on `next`, or sends an anonymous request to the app's login page.
pub(super) fn external_login(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current_realm(cx).ok_or_else(topcoat::router::error::not_found)?;
        let external = panel
            .auth
            .external()
            .ok_or_else(topcoat::router::error::not_found)?;
        let next = next_from_query(cx).unwrap_or_default();
        let user = external
            .current_user(cx)
            .await
            .map_err(infrastructure_failure)?;
        let Some(user) = user else {
            let back = with_next(login_url(cx), &next);
            let back: String = form_urlencoded::byte_serialize(back.as_bytes()).collect();
            return Err(redirect(external.login_url(cx, &back)).into());
        };
        // The app signed this user in, so unlike a login form the page can say why it stops.
        if !user.can_access_panel(&panel.prefix) {
            return no_access(cx, external.logout_url(cx)).await;
        }
        if let Err(error) = sweep_expired_sessions(cx).await {
            tracing::error!(error = %error, "expired-session sweep failed");
        }
        let session = fresh_session(cx).await?;
        record(cx, &session, &*user, panel).await?;
        let target = if next.is_empty() {
            panel_root(cx)
        } else {
            next
        };
        see_other(target).into_response(cx)
    })
}

/// The 403 page for an app user the panel refuses: they hold no panel session to sign out of, so
/// its button leaves for the app.
async fn no_access(cx: &Cx, leave: String) -> topcoat::Result<topcoat::router::response::Response> {
    let link = view! {
        cx =>
        <a
            href=(leave)
            class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))
            data-external-sign-out=""
        >
            icon(data: tablo_ui::icons::LOG_OUT)
            (crate::t!(cx, "tablo-shell-sign-out"))
        </a>
    }
    .boxed();
    refused(cx, link).await
}

/// What an [`Auth::external`](crate::Auth::external) panel may not declare: the features that need
/// Tablo to own the account's credentials.
pub(super) fn declaration_errors(auth: &AuthConfig) -> Vec<DeclarationErrorKind> {
    if auth.external().is_none() {
        return Vec::new();
    }
    [
        ("Auth::registration", auth.registrar().is_some()),
        ("Auth::password_reset", auth.reset().is_some()),
        ("Auth::email_verification", auth.verification().is_some()),
        ("Auth::two_factor", auth.two_factor_config().is_some()),
    ]
    .into_iter()
    .filter(|(_, declared)| *declared)
    .map(|(feature, _)| DeclarationErrorKind::ExternalAuthFeature { feature })
    .collect()
}
