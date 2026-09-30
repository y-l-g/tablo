//! Tenancy via the `Cx` scoped value `Tenant(id)`.
//!
//! The framework owns the tenant *filter*, not only the tenant gate. For a
//! resource whose `requires_tenant()` is `true`, every loader runs
//! [`scoped_query`](crate::resource::scoped_query) — the resource's own
//! [`query`](crate::resource::Resource::query) with `tenant_id = <tenant>`
//! ANDed onto it — and the column is discovered from the model's schema here
//! (`derived_tenant_filter`, the default body of `Resource::tenant_scope`). A
//! gated resource therefore cannot serve unscoped rows by forgetting an
//! override, and `Resource::query` stays the app's *non-tenant* scoping seam.
//!
//! Discovery is narrow and fails closed: the model must declare a field whose
//! application name is `tenant_id` and whose type is a UUID. Anything else
//! reads as "no tenant column", and a gated resource that hits that is refused
//! by `Panel::build` at boot. A predicate that is only `None` for some tenants
//! still answers an error naming the resource rather than querying unscoped.
//!
//! The authenticated user's tenant is the production source: the auth layer
//! injects `Tenant` into the request `Cx` when the logged-in user carries one.
//! A server-set `Tenant` request extension takes precedence, so app middleware
//! and `Router::handle` tests can override it deliberately. No request header
//! supplies a tenant: learning another tenant's UUID does not make anyone that
//! tenant.

use toasty::stmt::Expr;
use topcoat::context::{Cx, try_request_context};

/// Request-scoped tenant identifier.
///
/// The auth layer sets it via `cx.with(Tenant(id))` from the logged-in user;
/// server code and tests may also carry it as a request extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tenant(pub uuid::Uuid);

/// Returns the tenant id from `cx`, if present.
///
/// Checks a `Tenant` request extension first (a server-set override — the
/// deliberate seam app middleware and `Router::handle` tests use), then the
/// `Tenant` scoped value the auth layer injects from the authenticated user.
/// No request header is consulted.
pub fn tenant_id(cx: &Cx) -> Option<uuid::Uuid> {
    if let Some(parts) = try_request_context::<http::request::Parts>(cx)
        && let Some(t) = parts.extensions.get::<Tenant>()
    {
        return Some(t.0);
    }
    try_request_context::<Tenant>(cx).map(|t| t.0)
}

/// Requires a tenant, returning an error if missing (for tenancy-gated resources).
pub fn require_tenant(cx: &Cx) -> Result<uuid::Uuid, topcoat::Error> {
    tenant_id(cx).ok_or_else(|| topcoat::router::error::forbidden().into())
}

/// The application name of the column the framework scopes a gated resource by.
///
/// The convention is public — it is the contract a model signs up to when its
/// resource declares
/// [`requires_tenant`](crate::resource::Resource::requires_tenant) — and this
/// is the one place the discovery reads it.
const TENANT_FIELD: &str = "tenant_id";

/// Position of `M`'s tenant column in its own schema, or `None` when `M`
/// declares none the framework can recognize.
///
/// Found by **name and type** over [`Model::schema`]'s field list: a primitive
/// field whose application name is `tenant_id` and whose type is
/// [`toasty::stmt::Type::Uuid`]. The index is taken from the field's own
/// [`FieldId`](toasty::schema::app::FieldId) — the index
/// [`Model::path_field`] addresses — rather than from the position in the
/// vector, so the filter and the generated `tenant_id()` accessor cannot drift.
///
/// Name-based discovery is the fragile half, so a miss is `None` and every
/// caller treats it as *fail closed*: nothing falls back to the unscoped query,
/// and nothing guesses. A `tenant_id` declared as `String`, or a tenant column
/// spelled any other way, is invisible here on purpose — comparing a UUID
/// against it would be a driver-level type error at best and a cross-tenant
/// match at worst.
pub(crate) fn tenant_field_index<M: toasty::schema::Model>() -> Option<usize> {
    M::schema()
        .fields()
        .iter()
        .find(|field| {
            field.name.app.as_deref() == Some(TENANT_FIELD)
                && field
                    .ty
                    .as_primitive()
                    .is_some_and(|primitive| primitive.ty.is_uuid())
        })
        .map(|field| field.id.index)
}

/// The form key of `M`'s tenant column, when [`tenant_field_index`] finds one.
pub(crate) fn tenant_field_name<M: toasty::schema::Model>() -> Option<String> {
    let index = tenant_field_index::<M>()?;
    M::schema()
        .fields()
        .get(index)
        .and_then(|field| field.name.app.clone())
}

/// The **derived** `tenant_id = tenant` over `M`, or `None` when
/// [`tenant_field_index`] finds no tenant column.
///
/// This is the default body of
/// [`Resource::tenant_scope`](crate::resource::Resource::tenant_scope) — the
/// public hook a resource overrides when its rows inherit their tenant instead
/// of carrying one — so the name says *derived*: it is the name-based
/// convenience, not the only way to scope a gated resource.
///
/// Built generically on purpose: `Model::path_field` addresses the column by
/// index and `Value::Uuid` types the comparison, so no generated accessor — and
/// no per-resource copy of the filter — is needed.
pub(crate) fn derived_tenant_filter<M: toasty::schema::Model>(
    tenant: uuid::Uuid,
) -> Option<Expr<bool>> {
    let index = tenant_field_index::<M>()?;
    Some(M::path_field::<uuid::Uuid>(index).eq(tenant))
}

#[cfg(test)]
mod tests;
