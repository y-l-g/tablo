//! The panel gate: auth and tenant enforcement plus the prefix-derived URLs
//! every handler builds on.

use topcoat::{Result, context::Cx};

use crate::resource::Resource;

/// The mount prefix of the [`Panel`] that built this Router (e.g. `/admin`).
/// Installed by [`Panel::build`](super::Panel::build) so generic handlers can derive every
/// resource URL as `{prefix}/{slug}` — correct by construction even when a
/// table renders away from its own list route — instead of sniffing the
/// request path (item 6).
#[derive(Debug, Clone)]
pub(crate) struct PanelPrefix(pub(crate) String);

/// Demo/deployment hint rendered under the login form (auth feature).
#[cfg(feature = "auth")]
#[derive(Debug, Clone)]
pub(crate) struct LoginHint(pub(crate) String);

/// Defense-in-depth companion to the auth gate (ADR-0013): every
/// panel handler and the live-search shard re-check the resolved user, so a
/// missing or mis-mounted gate cannot silently open a handler. A no-op when
/// the panel explicitly disabled auth.
#[cfg(feature = "auth")]
pub(crate) fn enforce_auth(cx: &Cx) -> Result<(), topcoat::Error> {
    if crate::auth::enforced(cx) {
        crate::auth::require_authenticated(cx)?;
    }
    Ok(())
}

/// Auth compiled out: the gate does not exist either, so nothing to enforce.
#[cfg(not(feature = "auth"))]
pub(crate) fn enforce_auth(_cx: &Cx) -> Result<(), topcoat::Error> {
    Ok(())
}

/// Enforce tenancy gating for resources that require it.
///
/// Wired into every resource handler; a no-op unless the resource overrides
/// `Resource::requires_tenant`. Fails closed (403) when no tenant is present
/// instead of serving unscoped rows.
pub(crate) fn enforce_tenant<R: Resource>(cx: &Cx) -> Result<(), topcoat::Error> {
    if R::requires_tenant() {
        crate::tenancy::require_tenant(cx)?;
    }
    Ok(())
}

/// The gate every resource handler runs first: the authenticated user, then
/// the resource's tenant. A no-op when auth is compiled out and the resource
/// declares no tenant.
pub(crate) fn gate<R: Resource>(cx: &Cx) -> Result<(), topcoat::Error> {
    enforce_auth(cx)?;
    enforce_tenant::<R>(cx)
}

/// The panel's URL prefix: the [`PanelPrefix`] app context installed by
/// [`Panel::build`](super::Panel::build), else the request path's first segment, else `/admin`.
///
/// A bare `CxTestBuilder` installs no prefix, so a test rendering under
/// `/admin/...` still derives `/admin`.
pub(crate) fn panel_prefix(cx: &Cx) -> String {
    topcoat::context::try_app_context::<PanelPrefix>(cx)
        .map(|p| p.0.clone())
        .unwrap_or_else(|| {
            let path = topcoat::router::request::uri(cx).path().to_string();
            path.split('/')
                .nth(1)
                .filter(|s| !s.is_empty())
                .map(|s| format!("/{s}"))
                .unwrap_or_else(|| "/admin".to_string())
        })
}

/// The list URL for a resource: `{panel prefix}/{slug}`.
pub(crate) fn list_url(cx: &Cx, slug: &str) -> String {
    format!("{}/{slug}", panel_prefix(cx))
}

#[cfg(test)]
mod tests;
