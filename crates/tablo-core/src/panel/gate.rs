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
mod tests {
    #[cfg(feature = "auth")]
    use toasty::Db;
    #[cfg(feature = "auth")]
    use topcoat::router::Body;

    #[cfg(feature = "auth")]
    use super::super::search::TABLE_SEARCH_PATH;
    use super::*;
    #[cfg(feature = "auth")]
    use crate::Panel;

    #[test]
    fn list_url_prefers_panel_prefix_over_request_path() {
        use topcoat::context::CxTestBuilder;

        // With the panel prefix installed, the resource URL is derived from
        // the declaration — even on a path that is not the list route.
        let (parts, ()) = http::Request::builder()
            .uri("/admin/users/42/edit")
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new()
            .request_context(parts)
            .app_context(PanelPrefix("/admin".to_string()))
            .build();
        assert_eq!(list_url(&cx, "users"), "/admin/users");

        let (parts, ()) = http::Request::builder()
            .uri("/backoffice/users")
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new()
            .request_context(parts)
            .app_context(PanelPrefix("/backoffice".to_string()))
            .build();
        assert_eq!(list_url(&cx, "users"), "/backoffice/users");

        // Without a panel prefix (bare test builder), fall back to the
        // request path's first segment.
        let (parts, ()) = http::Request::builder()
            .uri("/admin/users/42/edit")
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new().request_context(parts).build();
        assert_eq!(list_url(&cx, "users"), "/admin/users");
    }

    /// The named runtime endpoints answer the gate: a request
    /// without a session to a shard's fixed path is refused with 401, not a
    /// login redirect and not the shard's content. The path is stable, so the
    /// refusal is the only thing that keeps it from being probed.
    #[cfg(feature = "auth")]
    #[tokio::test]
    async fn named_shard_endpoints_answer_401_without_a_session() {
        let db = Db::builder()
            .models(toasty::models!(
                crate::auth::AdminUser,
                crate::auth::AuthSession
            ))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let router = Panel::new("admin")
            .app_context(db)
            .auth(crate::Auth::password())
            .build()
            .expect("panel builds");

        for path in [TABLE_SEARCH_PATH, crate::notification::LIVE_TOASTER_PATH] {
            let request = http::Request::builder()
                .method(http::Method::POST)
                .uri(path)
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}".to_owned()))
                .unwrap();
            let response = router.handle(request).await;
            assert_eq!(
                response.status(),
                http::StatusCode::UNAUTHORIZED,
                "an unauthenticated shard request must answer 401: {path}"
            );
        }
    }
}
