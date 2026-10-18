//! The layers that resolve a session into request `Cx` and answer
//! fail-closed: one per panel prefix, one over Topcoat's runtime path.

use std::sync::Arc;

use topcoat::{
    context::Cx,
    icon::icon,
    router::{
        Body, Layer, LayerFuture, Next, Path, PathBuf,
        error::{forbidden, redirect, unauthorized},
        request::{method, uri},
        response::{IntoResponse, Response},
    },
    view::{ViewExt, attributes, view},
};

use super::{
    SignedIn, email_change,
    login::{login_url, logout_url},
    page::{AuthPage, render},
    page_load,
    password_reset::{forgot_url, offered as reset_offered, reset_url},
    registration::{offered, register_url},
    session::{resolve, session_row, session_user},
    tenant_registration, two_factor, unauthenticated_error, verification,
};
use crate::panel::{
    Panel,
    error_page::answer,
    route_path,
    state::{CurrentPanel, PanelState, Panels, panels},
};

/// Resolves the panel prefix's session into request `Cx` and answers fail-closed
/// without a permitted user.
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
            let verifies = self.panel.auth.verification().is_some();
            // A verification link acts on GET; an email-change link shows a button that posts.
            let confirm = verifies
                && path == verification::confirm_url(&cx)
                && matches!(*method(&cx), http::Method::GET | http::Method::HEAD);
            let email_change = verifies
                && (path == email_change::confirm_url(&cx)
                    || path == email_change::cancel_url(&cx));
            let challenge = self.panel.auth.two_factor_config().is_some()
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
                    && let Some(signed) = resolve(&cx, &self.panel, authenticator).await?
                    && signed.user.can_access_panel()
                {
                    return Err(redirect(self.panel.prefix.clone()).into());
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
            let signed = resolve(&cx, &self.panel, authenticator).await?;
            // A user who owes a step, such as verifying their address, reaches only the step's
            // page and the logout route.
            if let Some(signed) = &signed
                && signed.user.can_access_panel()
                && !logout_route
                && let Some(step) = owed_step(&cx, &self.panel, signed).await?
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
                    let cx = cx.with(signed);
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
async fn owed_step(
    cx: &Cx,
    panel: &PanelState,
    signed: &SignedIn,
) -> topcoat::Result<Option<Step>> {
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
    Ok(signed.user.can_access_panel() && owed_step(cx, &signed.panel, signed).await?.is_none())
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
                "Sign out"
            )
        </form>
    }
    .boxed();
    let page = render(
        cx,
        AuthPage {
            title: "Access denied",
            description: format!("Your account cannot open {}.", Panel::brand_name(cx)),
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
            let Some(panels) = panels(cx) else {
                return next.run(cx, body).await;
            };
            let declaring: Vec<_> = panels.declaring(uri(cx).path()).collect();
            let signed = if panels.any_gates() {
                signed_in(cx, panels).await?
            } else {
                None
            };
            let admitted = match &signed {
                Some(signed) => admitted(cx, signed).await?,
                None => false,
            };
            if declaring.is_empty() {
                return match signed {
                    Some(signed) if admitted => {
                        let panel = CurrentPanel(Arc::clone(&signed.panel));
                        next.run(&cx.with_many((signed, panel)), body).await
                    }
                    Some(_) => Err(forbidden().into()),
                    None if panels.all_gate() => Err(unauthorized().into()),
                    None => next.run(cx, body).await,
                };
            }
            // A session another panel signed in is no session for the declaring panels.
            match signed.filter(|signed| declaring.iter().any(|p| Arc::ptr_eq(p, &signed.panel))) {
                Some(signed) if admitted => {
                    let panel = CurrentPanel(Arc::clone(&signed.panel));
                    next.run(&cx.with_many((signed, panel)), body).await
                }
                Some(_) => Err(forbidden().into()),
                None => match declaring.iter().find(|panel| !panel.gates()) {
                    Some(open) => {
                        next.run(&cx.with(CurrentPanel(Arc::clone(open))), body)
                            .await
                    }
                    None => Err(unauthorized().into()),
                },
            }
        })
    }
}

/// The user the request's session signed in, through the auth of the panel that issued it.
async fn signed_in(cx: &Cx, panels: &Panels) -> topcoat::Result<Option<SignedIn>> {
    let Some(row) = session_row(cx).await? else {
        return Ok(None);
    };
    match panels
        .by_prefix(&row.panel)
        .and_then(|panel| Some((panel, panel.auth.authenticator()?)))
    {
        Some((panel, authenticator)) => session_user(cx, row, panel, authenticator).await,
        None => Ok(None),
    }
}
