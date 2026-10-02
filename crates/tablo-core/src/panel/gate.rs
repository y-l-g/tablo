//! The panel gate: auth and tenant enforcement plus the prefix-derived URLs
//! every handler builds on.

use topcoat::{Result, context::Cx};

use super::state::current;
use crate::resource::{RETURN_PARAM, Resource};

/// Rejects tenant-scoped requests without a tenant, never serving unscoped rows.
pub(crate) fn enforce_tenant<R: Resource>(cx: &Cx) -> Result<(), topcoat::Error> {
    if R::tenancy().is_scoped() {
        crate::tenancy::require_tenant(cx)?;
    }
    Ok(())
}

pub(crate) fn gate<R: Resource>(cx: &Cx) -> Result<(), topcoat::Error> {
    crate::auth::guard(cx)?;
    enforce_tenant::<R>(cx)
}

/// The request's panel prefix, else the request path's first segment, else `/admin`.
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

pub(crate) fn list_url(cx: &Cx, slug: &str) -> String {
    format!("{}/{slug}", panel_prefix(cx))
}

/// Returns the validated `?return=` target under the panel prefix, else `None`.
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

pub(crate) fn landing_url(cx: &Cx, slug: &str) -> String {
    return_target(cx).unwrap_or_else(|| list_url(cx, slug))
}

#[cfg(test)]
mod tests;
