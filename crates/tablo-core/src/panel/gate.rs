//! The panel gate: auth and tenant enforcement plus the prefix-derived URLs
//! every handler builds on.

use topcoat::{Result, context::Cx};

use super::state::current;
use crate::resource::{RETURN_PARAM, Resource};

/// Enforce tenancy for a tenant-scoped resource: 403 when the request has no
/// tenant, instead of serving unscoped rows. A no-op for a resource whose
/// [`tenancy`](Resource::tenancy) is none.
pub(crate) fn enforce_tenant<R: Resource>(cx: &Cx) -> Result<(), topcoat::Error> {
    if R::tenancy().is_scoped() {
        crate::tenancy::require_tenant(cx)?;
    }
    Ok(())
}

/// The gate every resource handler runs first: the panel's sign-in
/// ([`auth::guard`](crate::auth::guard)), then the resource's tenant.
pub(crate) fn gate<R: Resource>(cx: &Cx) -> Result<(), topcoat::Error> {
    crate::auth::guard(cx)?;
    enforce_tenant::<R>(cx)
}

/// The request's panel prefix, else the request path's first segment, else
/// `/admin`.
///
/// Every resource URL derives from it as `{prefix}/{slug}`, correct by
/// construction even when a table renders away from its own list route. A
/// bare `CxTestBuilder` mounts no panel, so a test rendering under
/// `/admin/...` still derives `/admin`.
pub(crate) fn panel_prefix(cx: &Cx) -> String {
    current(cx)
        .map(|panel| panel.prefix.clone())
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

/// The request's `?return=` target, when it is a same-origin path under the
/// panel prefix ([`safe_next`](crate::auth::safe_next), no `.` or `..`
/// segment — raw or percent-encoded — that the browser would resolve out of
/// the prefix, then the prefix check): a relation table's actions carry it so
/// a write lands back on the record page it started from. Anything else is
/// ignored, never followed.
pub(crate) fn return_target(cx: &Cx) -> Option<String> {
    let query = topcoat::router::request::uri(cx).query()?;
    let prefix = panel_prefix(cx);
    form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == RETURN_PARAM)
        .and_then(|(_, value)| crate::auth::safe_next(&value).map(str::to_string))
        .filter(|target| !has_dot_segment(target))
        .filter(|target| {
            target
                .strip_prefix(prefix.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?']))
        })
}

/// Whether `target`'s path holds a `.` or `..` segment, spelled raw or with
/// `%2e`.
fn has_dot_segment(target: &str) -> bool {
    let path = target.split(['?', '#']).next().unwrap_or_default();
    path.split('/').any(|segment| {
        let segment = segment.to_ascii_lowercase().replace("%2e", ".");
        segment == "." || segment == ".."
    })
}

/// Where a write on the resource `slug` lands: the request's
/// [`return_target`], else the resource's list.
pub(crate) fn landing_url(cx: &Cx, slug: &str) -> String {
    return_target(cx).unwrap_or_else(|| list_url(cx, slug))
}

#[cfg(test)]
mod tests;
