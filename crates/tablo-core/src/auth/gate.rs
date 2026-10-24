//! The layers that resolve a session into request `Cx` and answer
//! fail-closed: one per panel prefix, one over Topcoat's runtime path.

use std::sync::Arc;

use topcoat::{
    context::Cx,
    icon::icon,
    router::{
        Body, Layer, LayerFuture, Next, Path, PathBuf,
        error::{forbidden, not_found, redirect, unauthorized},
        request::{method, uri},
        response::{IntoResponse, Response, response_headers},
    },
    view::{ViewExt, attributes, view},
};

use super::{
    SignedIn, email_change,
    login::{login_url, logout_url},
    page::{AuthPage, render},
    page_load,
    password_reset::{forgot_url, offered as reset_offered, reset_url},
    realm::{Realm, host, hosted, realms},
    registration::{offered, register_url},
    session::{resolve, session_row, session_user},
    tenant_registration, two_factor, unauthenticated_error, verification,
};
use crate::naming::route_path;

/// Resolves the panel prefix's session into request `Cx` and answers fail-closed
/// without a permitted user.
pub(crate) struct PanelGate {
    path: PathBuf,
    realm: Arc<Realm>,
}

impl PanelGate {
    /// The gate of `realm`'s panel, which the installed [`Host`](super::Host) enters.
    pub(crate) fn new(realm: Arc<Realm>) -> Self {
        Self {
            path: route_path(&realm.prefix),
            realm,
        }
    }
}

impl Layer for PanelGate {
    fn path(&self) -> Option<&Path> {
        Some(&self.path)
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let Some((host, cx)) =
                host(cx).and_then(|host| Some((host, host.enter(cx, &self.realm)?)))
            else {
                return Err(not_found().into());
            };
            let answer = |cx: &Cx, error| host.answer(cx, error);
            // A page of a panel offering several locales reads the browser's languages, so a
            // shared cache must not serve one browser's page to another.
            if self.realm.catalog.locales().len() > 1 {
                response_headers(&cx).append(
                    http::header::VARY,
                    http::HeaderValue::from_static("accept-language"),
                );
            }
            let Some(authenticator) = self.realm.auth.authenticator() else {
                return next
                    .run(&cx, body)
                    .await
                    .map_err(|error| answer(&cx, error));
            };
            // The login page, and the sign-up, password reset and verification link pages when
            // the panel offers them, must answer while logged out. Only the methods their routes
            // serve (GET, its HEAD, and POST) pass: an app route at the same
            // path under another method stays gated.
            let path = uri(&cx).path();
            let sign_up = offered(&cx) && path == register_url(&cx);
            let reset = reset_offered(&cx) && (path == forgot_url(&cx) || path == reset_url(&cx));
            let verifies = self.realm.auth.verification().is_some();
            // A verification link acts on GET; an email-change link shows a button that posts.
            let confirm = verifies
                && path == verification::confirm_url(&cx)
                && matches!(*method(&cx), http::Method::GET | http::Method::HEAD);
            let email_change = verifies
                && (path == email_change::confirm_url(&cx)
                    || path == email_change::cancel_url(&cx));
            let challenge = self.realm.auth.two_factor_config().is_some()
                && path == two_factor::challenge_url(&cx);
            let public =
                path == login_url(&cx) || sign_up || reset || confirm || email_change || challenge;
            if public
                && matches!(
                    *method(&cx),
                    http::Method::GET | http::Method::HEAD | http::Method::POST
                )
            {
                // A signed-in user has an account already: the sign-up page sends them to the
                // panel instead of offering to replace their session with a new one.
                if sign_up
                    && matches!(*method(&cx), http::Method::GET | http::Method::HEAD)
                    && let Some(signed) = resolve(&cx, &self.realm, authenticator).await?
                    && signed.user.can_access_panel()
                {
                    return Err(redirect(self.realm.prefix.clone()).into());
                }
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
            let signed = resolve(&cx, &self.realm, authenticator).await?;
            // A user who owes a step, such as verifying their address, reaches only the step's
            // page and the logout route.
            if let Some(signed) = &signed
                && signed.user.can_access_panel()
                && !logout_route
                && let Some(step) = owed_step(&cx, &self.realm, signed).await?
                && !step.serves(&cx)
            {
                return if page_load(&cx) {
                    Err(redirect(step.url(&cx)).into())
                } else {
                    Err(forbidden().into())
                };
            }
            match signed {
                Some(signed) if signed.user.can_access_panel() || logout_route => {
                    let cx = signed.sign_into(&cx);
                    next.run(&cx, body)
                        .await
                        .map_err(|error| answer(&cx, error))
                }
                // Authenticated but not permitted: 403, on a page that lets them sign out
                // rather than inside a shell they may not see. The logout route is answered
                // above.
                Some(_) if page_load(&cx) => no_access(&cx).await,
                Some(_) => Err(forbidden().into()),
                // Pages redirect to the login route with a validated `next`;
                // runtime endpoints, non-GET requests, and page re-runs
                // answer 401.
                None => Err(unauthenticated_error(&cx)),
            }
        })
    }
}

/// What a signed-in user with panel access must do before the panel serves them anything else.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// Prove they read the address they store.
    VerifyEmail,
    /// Set up the authenticator app the panel requires.
    SetUpTwoFactor,
    /// Create a tenant to act for.
    RegisterTenant,
}

impl Step {
    /// Where the step's page lives.
    fn url(self, cx: &Cx) -> String {
        match self {
            Self::VerifyEmail => verification::notice_url(cx),
            Self::SetUpTwoFactor => two_factor::settings_url(cx),
            Self::RegisterTenant => tenant_registration::register_tenant_url(cx),
        }
    }

    /// Whether the request is one the step's page answers.
    fn serves(self, cx: &Cx) -> bool {
        uri(cx).path() == self.url(cx)
            && matches!(
                *method(cx),
                http::Method::GET | http::Method::HEAD | http::Method::POST
            )
    }
}

/// The step `signed` owes `panel`, if any: an unverified address first, then a required
/// authenticator, since registering a tenant is reaching the panel.
async fn owed_step(cx: &Cx, panel: &Realm, signed: &SignedIn) -> topcoat::Result<Option<Step>> {
    let user = signed.user.as_ref();
    Ok(if verification::pending(panel, user) {
        Some(Step::VerifyEmail)
    } else if two_factor::setup_pending(cx, panel, user).await? {
        Some(Step::SetUpTwoFactor)
    } else {
        tenant_registration::pending(panel, user).then_some(Step::RegisterTenant)
    })
}

/// Whether `signed` may reach any endpoint: they have panel access and owe no step.
async fn admitted(cx: &Cx, signed: &SignedIn) -> topcoat::Result<bool> {
    Ok(signed.user.can_access_panel() && owed_step(cx, &signed.realm, signed).await?.is_none())
}

/// Whether the session that signed the request in still signs in the same user, admitted, acting
/// for the same tenant: what a connected render checks before it shows anything new, since the
/// gate ran once, when the render started.
pub(crate) async fn still_signed_in(cx: &Cx) -> topcoat::Result<bool> {
    let Some(signed) = super::signed(cx) else {
        return Ok(false);
    };
    let Some(authenticator) = signed.realm.auth.authenticator() else {
        return Ok(true);
    };
    let Some(now) = resolve(cx, &signed.realm, authenticator).await? else {
        return Ok(false);
    };
    let tenant = crate::tenant_id(cx);
    let member =
        tenant.is_none_or(|tenant| now.user.tenants().iter().any(|m| m.tenant.get() == tenant));
    Ok(now.user.user_id() == signed.user.user_id()
        && now.tenant == signed.tenant
        && member
        && admitted(cx, &now).await?)
}

/// The page a signed-in user the panel refuses loads: a 403 saying so, with Sign out.
async fn no_access(cx: &Cx) -> topcoat::Result<Response> {
    let csrf = crate::csrf::ensure_token(cx);
    let logout = logout_url(cx);
    let form = view! {
        cx =>
        <form method="post" action=(logout)>
            (crate::csrf::field(cx, &csrf))
            tablo_ui::button(
                variant: tablo_ui::ButtonVariant::Outline,
                attrs: attributes! { type="submit" class="w-full" },
                icon(data: tablo_ui::icons::LOG_OUT)
                (crate::t!(cx, "tablo-shell-sign-out"))
            )
        </form>
    }
    .boxed();
    let page = render(
        cx,
        AuthPage {
            title: crate::t!(cx, "tablo-error-forbidden"),
            description: crate::t!(
                cx,
                "tablo-gate-description",
                brand = hosted(cx)?.brand_name(cx)
            ),
            status: Some(http::StatusCode::FORBIDDEN),
            form,
            switch: None,
            note: None,
        },
    )
    .await?;
    page.single().await?.into_response(cx)
}

/// Guards Topcoat's runtime endpoints shared by all panels.
///
/// An endpoint a panel declared with [`Panel::endpoint`] runs as that panel: only its sessions
/// sign in, and it answers 401 without one when the panel is gated. Any other endpoint resolves
/// the session through its issuing panel's auth; it answers 401 when every mounted panel is
/// gated, otherwise passes on for each panel's shard to re-check.
pub(crate) struct RuntimeGate {
    path: PathBuf,
}

impl RuntimeGate {
    /// The gate over every panel the installed [`Host`](super::Host) mounts.
    pub(crate) fn new() -> Self {
        Self {
            path: route_path(crate::topcoat_compat::RUNTIME_PREFIX),
        }
    }
}

impl Layer for RuntimeGate {
    fn path(&self) -> Option<&Path> {
        Some(&self.path)
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let realms = realms(cx);
            let Some(host) = host(cx).filter(|_| !realms.is_empty()) else {
                return next.run(cx, body).await;
            };
            let declaring = host.declaring(cx, uri(cx).path());
            let signed = if realms.iter().any(|realm| realm.gates()) {
                signed_in(cx, &realms).await?
            } else {
                None
            };
            // The request runs as the panel `realm` serves; a realm no panel serves answers 404.
            let enter = |cx: &Cx, realm: &Arc<Realm>| host.enter(cx, realm).ok_or_else(not_found);
            let admitted = match &signed {
                Some(signed) => admitted(cx, signed).await?,
                None => false,
            };
            if declaring.is_empty() {
                return match signed {
                    Some(signed) if admitted => {
                        let entered = enter(cx, &signed.realm)?;
                        let cx = signed.sign_into(&entered);
                        next.run(&cx, body).await
                    }
                    Some(_) => Err(forbidden().into()),
                    None if realms.iter().all(|realm| realm.gates()) => Err(unauthorized().into()),
                    None => next.run(cx, body).await,
                };
            }
            // A session another panel signed in is no session for the declaring panels.
            match signed.filter(|signed| declaring.iter().any(|p| Arc::ptr_eq(p, &signed.realm))) {
                Some(signed) if admitted => {
                    let entered = enter(cx, &signed.realm)?;
                    let cx = signed.sign_into(&entered);
                    next.run(&cx, body).await
                }
                Some(_) => Err(forbidden().into()),
                None => match declaring.iter().find(|realm| !realm.gates()) {
                    Some(open) => next.run(&enter(cx, open)?, body).await,
                    None => Err(unauthorized().into()),
                },
            }
        })
    }
}

/// The user the request's session signed in, through the auth of the realm that issued it.
async fn signed_in(cx: &Cx, realms: &[Arc<Realm>]) -> topcoat::Result<Option<SignedIn>> {
    let Some(row) = session_row(cx).await? else {
        return Ok(None);
    };
    match realms
        .iter()
        .find(|realm| realm.prefix == row.panel)
        .and_then(|realm| Some((realm, realm.auth.authenticator()?)))
    {
        Some((realm, authenticator)) => session_user(cx, row, realm, authenticator).await,
        None => Ok(None),
    }
}
