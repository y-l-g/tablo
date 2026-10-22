//! The panel gate: auth and tenant enforcement plus the prefix-derived URLs
//! every handler builds on.

use std::sync::Arc;

use topcoat::{Result, context::Cx};

use super::state::current;
use crate::{
    panel::url::RETURN_PARAM,
    resource::{Mounted, Resource, mounted, require_mounted},
};

/// Rejects tenant-scoped requests without a tenant, never serving unscoped rows.
pub(crate) fn enforce_tenant<R: Resource>(cx: &Cx, resource: &Mounted<R>) -> Result<()> {
    if resource.tenancy.is_scoped() {
        crate::tenancy::require_tenant(cx)?;
    }
    Ok(())
}

/// Signs in and scopes the request for `R`, answering `R` as the request's panel mounted it.
pub(crate) fn gate<R: Resource>(cx: &Cx) -> Result<Arc<Mounted<R>>> {
    crate::auth::guard(cx)?;
    let resource = require_mounted::<R>(cx)?;
    enforce_tenant(cx, &resource)?;
    Ok(resource)
}

/// Whether the current request may open `R`'s list: the request's panel mounts `R`, and
/// sign-in, tenant scope, and [`Ability::ViewAny`](crate::Ability::ViewAny) allow it.
pub fn can_list<R: Resource>(cx: &Cx) -> bool {
    crate::auth::guard(cx).is_ok()
        && mounted::<R>(cx).is_some_and(|resource| {
            enforce_tenant(cx, &resource).is_ok() && resource.can(cx, crate::Ability::ViewAny)
        })
}

/// Whether the current request may open the page `P`: the request's panel mounts `P`, and sign-in
/// and [`PageDef::access`](crate::PageDef::access) allow it, the checks the page's `GET` makes.
///
/// A form a page renders posts to an app route, which [`PageDef::access`](crate::PageDef::access)
/// does not cover; the route asks `can_open::<P>(cx)` itself.
pub fn can_open<P: crate::Page>(cx: &Cx) -> bool {
    crate::auth::guard(cx).is_ok()
        && super::pages::mounted_page::<P>(cx).is_some_and(|page| page.admits(cx))
}

/// The request's panel prefix, else the request path's first segment, else `/admin`.
pub(crate) fn panel_prefix(cx: &Cx) -> String {
    current(cx).map_or_else(
        || crate::naming::path_prefix(topcoat::router::request::uri(cx).path()),
        |panel| panel.realm.prefix.clone(),
    )
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

/// The validated `?return=` target, else `list_url`.
pub(crate) fn landing_url(cx: &Cx, list_url: &str) -> String {
    return_target(cx).unwrap_or_else(|| list_url.to_string())
}

#[cfg(test)]
mod tests;
