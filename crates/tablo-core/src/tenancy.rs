//! Tenancy: which tenant a request acts for, and how a resource's rows belong
//! to one.
//!
//! A resource declares its [`Tenancy`]. A scoped one requires a tenant in every
//! handler, and the framework ANDs `<lens> = <tenant>` onto every loader
//! through [`scoped_query`](crate::resource::scoped_query), so
//! [`query`](crate::resource::Resource::query) stays the resource's
//! *non-tenant* scoping seam and no loader can drop the filter by omission.
//!
//! [`tenant_id`] answers the request's tenant: the tenant the panel's
//! signed-in user carries, unless a server-set `Tenant` request extension or
//! a `Tenant` scoped value overrides it, so app middleware and
//! `Router::handle` tests can set it deliberately. No request header supplies
//! a tenant: learning another tenant's UUID does not make anyone that tenant.

use toasty::stmt::{Expr, IntoExpr};
use topcoat::context::{Cx, try_request_context};

use crate::schema::FieldLens;

/// Request-scoped tenant identifier.
///
/// App code may put it on the `Cx` with `cx.with(Tenant(id))`, and server code
/// and tests may carry it as a request extension; either wins over the signed-in
/// user's tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tenant(pub uuid::Uuid);

/// Returns the tenant id from `cx`, if present.
///
/// Checks a `Tenant` request extension first (a server-set override — the
/// deliberate seam app middleware and `Router::handle` tests use), then a
/// `Tenant` scoped value app code put on the `Cx`, then the tenant of the
/// panel's signed-in [`current_user`](crate::auth::current_user). No request
/// header is consulted.
pub fn tenant_id(cx: &Cx) -> Option<uuid::Uuid> {
    if let Some(parts) = try_request_context::<http::request::Parts>(cx)
        && let Some(t) = parts.extensions.get::<Tenant>()
    {
        return Some(t.0);
    }
    if let Some(t) = try_request_context::<Tenant>(cx) {
        return Some(t.0);
    }
    crate::auth::current_user(cx).and_then(|user| user.tenant_id)
}

/// Requires a tenant, returning an error if missing (for tenancy-gated resources).
pub fn require_tenant(cx: &Cx) -> Result<uuid::Uuid, topcoat::Error> {
    tenant_id(cx).ok_or_else(|| topcoat::router::error::forbidden().into())
}

/// How a resource's rows belong to a tenant.
///
/// [`Resource::tenancy`](crate::resource::Resource::tenancy) returns one. A
/// scoped tenancy names, by lens, the tenant UUID each row is filtered on:
///
/// ```ignore
/// fn tenancy() -> Tenancy<Post> {
///     Tenancy::column(Post::fields().tenant_id())
/// }
///
/// fn tenancy() -> Tenancy<Comment> {
///     Tenancy::via(Comment::fields().post().tenant_id())
/// }
/// ```
///
/// A scoped resource answers 403 to a request with no tenant, in every handler,
/// instead of serving unscoped rows or writing rows with no tenant.
pub struct Tenancy<M> {
    scope: Scope,
    _model: std::marker::PhantomData<fn() -> M>,
}

/// A resource's own tenant column: the field the create stamps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TenantColumn {
    /// The field's index in its model.
    pub(crate) index: usize,
    /// The field's name, which is also its form key.
    pub(crate) name: String,
}

/// `<lens> = tenant` for one tenant.
type TenantFilter = Box<dyn Fn(uuid::Uuid) -> Expr<bool> + Send + Sync>;

enum Scope {
    None,
    /// The model's own column, resolved when declared: its field, or why the
    /// lens names none.
    Column {
        filter: TenantFilter,
        field: Result<TenantColumn, String>,
    },
    Via(TenantFilter),
}

impl<M: toasty::schema::Model + 'static> Tenancy<M> {
    fn scoped(scope: Scope) -> Self {
        Self {
            scope,
            _model: std::marker::PhantomData,
        }
    }

    /// Rows belong to no tenant: the resource is served as
    /// [`query`](crate::resource::Resource::query) states it, to every
    /// request. The default.
    pub fn none() -> Self {
        Self::scoped(Scope::None)
    }

    /// Rows carry their tenant in one UUID column of their own model, `Uuid`
    /// or `Option<Uuid>`.
    ///
    /// The framework filters every loader on it and stamps the request's
    /// tenant into it on create, so the record form must not claim it.
    /// Mounting the panel refuses a lens that is not a single field of the
    /// model.
    pub fn column<T>(lens: FieldLens<M, T>) -> Self
    where
        T: Send + Sync + 'static,
        M: Send + Sync,
        uuid::Uuid: IntoExpr<T>,
    {
        let field =
            crate::schema::lens_field(lens.clone(), &M::schema()).map(|field| TenantColumn {
                index: field.id.index,
                name: field.name.app_unwrap().to_string(),
            });
        Self::scoped(Scope::Column {
            filter: Box::new(move |tenant| lens.clone().eq(tenant)),
            field,
        })
    }

    /// Rows inherit their tenant through a relation: `lens` reaches the parent's
    /// tenant column, as `Comment::fields().post().tenant_id()` does.
    ///
    /// The framework filters every loader on it. It stamps nothing on create:
    /// a row's tenant is its parent's, so the foreign key the form writes must
    /// be a relationship field over the parent's resource, whose key the
    /// framework re-checks against the parent's tenant-scoped query inside the
    /// write.
    pub fn via<T>(lens: FieldLens<M, T>) -> Self
    where
        T: Send + Sync + 'static,
        M: Send + Sync,
        uuid::Uuid: IntoExpr<T>,
    {
        Self::scoped(Scope::Via(Box::new(move |tenant| lens.clone().eq(tenant))))
    }

    /// Whether the rows belong to a tenant, so every handler requires one.
    pub fn is_scoped(&self) -> bool {
        !matches!(self.scope, Scope::None)
    }

    /// `<lens> = tenant`, or `None` for an unscoped resource.
    pub(crate) fn filter(&self, tenant: uuid::Uuid) -> Option<Expr<bool>> {
        match &self.scope {
            Scope::None => None,
            Scope::Column { filter, .. } | Scope::Via(filter) => Some(filter(tenant)),
        }
    }

    /// The model's own tenant column for a [`column`](Self::column) tenancy,
    /// or why the lens names none.
    pub(crate) fn column_field(&self) -> Option<Result<&TenantColumn, &str>> {
        match &self.scope {
            Scope::Column { field, .. } => Some(field.as_ref().map_err(String::as_str)),
            Scope::None | Scope::Via(_) => None,
        }
    }
}

impl<M: toasty::schema::Model + 'static> Default for Tenancy<M> {
    fn default() -> Self {
        Self::none()
    }
}

#[cfg(test)]
mod tests;
