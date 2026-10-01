//! The layers that resolve a session into request `Cx` and answer
//! fail-closed: one per panel prefix, one over Topcoat's runtime path.

use std::sync::Arc;

use topcoat::{
    context::Cx,
    router::{
        Body, Layer, LayerFuture, Next, Path, PathBuf,
        error::{forbidden, unauthorized},
        request::{method, uri},
    },
};

use super::{
    RUNTIME_PREFIX,
    login::{login_url, logout_url},
    session::{resolve, session_row, session_user},
    unauthenticated_error,
};
use crate::panel::{
    route_path,
    state::{CurrentPanel, PanelState, panels},
};

/// The layer every request under a panel's prefix runs first (ADR-0013): it
/// puts the panel on the request, then, on a gated panel, resolves the
/// session into request `Cx` and answers fail-closed when the route has no
/// permitted user.
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
                return next.run(&cx, body).await;
            };
            // The login page must answer while logged out. Only the methods
            // the login routes serve (GET, its HEAD, and POST) pass: an app
            // route at the same path under another method stays gated.
            if uri(&cx).path() == login_url(&cx)
                && matches!(
                    *method(&cx),
                    http::Method::GET | http::Method::HEAD | http::Method::POST
                )
            {
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
            match resolve(&cx, &self.panel, authenticator).await? {
                Some(signed) if signed.user.can_access_panel() || logout_route => {
                    next.run(&cx.with(signed), body).await
                }
                // Authenticated but not permitted: 403, indistinguishable
                // from bad credentials at login (ADR-0013). The logout route
                // is answered above.
                Some(_) => Err(forbidden().into()),
                // Pages redirect to the login route with a validated `next`;
                // runtime endpoints, non-GET requests, and page re-runs
                // answer 401.
                None => Err(unauthenticated_error(&cx)),
            }
        })
    }
}

/// The layer over Topcoat's runtime endpoints (ADR-0013): shards and
/// procedures every panel's pages call, served at one path for all of them.
///
/// A session names the panel that issued it, so the gate resolves it through
/// that panel's auth and puts both the user and the panel on the request. A
/// request with no session answers 401 when every mounted panel is gated; with
/// an ungated panel mounted it passes on, and each panel's shard re-checks its
/// own panel's gate.
pub(crate) struct RuntimeGate {
    path: PathBuf,
}

impl RuntimeGate {
    pub(crate) fn new() -> Self {
        Self {
            path: route_path(RUNTIME_PREFIX),
        }
    }
}

impl Layer for RuntimeGate {
    fn path(&self) -> Option<&Path> {
        Some(&self.path)
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let Some(panels) = panels(cx).filter(|panels| panels.any_gates()) else {
                return next.run(cx, body).await;
            };
            let signed = match session_row(cx).await? {
                Some(row) => match panels
                    .by_prefix(&row.panel)
                    .and_then(|panel| Some((panel, panel.auth.authenticator()?)))
                {
                    Some((panel, authenticator)) => {
                        session_user(cx, row, panel, authenticator).await?
                    }
                    None => None,
                },
                None => None,
            };
            match signed {
                Some(signed) if signed.user.can_access_panel() => {
                    let panel = CurrentPanel(Arc::clone(&signed.panel));
                    next.run(&cx.with_many((signed, panel)), body).await
                }
                Some(_) => Err(forbidden().into()),
                None if panels.all_gate() => Err(unauthorized().into()),
                None => next.run(cx, body).await,
            }
        })
    }
}
