//! Scopes resources to the request tenant.
//!
//! [`tenant_id`] answers the request's tenant: one of the signed-in user's
//! [`tenants`](crate::auth::PanelUser::tenants) — the one the session selected
//! with the tenant switcher, else the first — unless a server-set `Tenant`
//! request extension or a `Tenant` scoped value overrides it, so app
//! middleware and `Router::handle` tests can set it deliberately. No request
//! header supplies a tenant: learning another tenant's UUID does not make
//! anyone that tenant.

use toasty::stmt::{Expr, IntoExpr, Path};
use topcoat::context::{Cx, try_request_context};

use crate::DeclarationErrorKind;

/// One tenant a user may act for: its id, and the name the tenant switcher
/// shows. A [`PanelUser`](crate::auth::PanelUser) lists its memberships in
/// [`tenants`](crate::auth::PanelUser::tenants).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Membership {
    pub tenant: uuid::Uuid,
    pub name: String,
}

impl Membership {
    /// The membership of `tenant`, shown as `name`.
    pub fn new(tenant: uuid::Uuid, name: impl Into<String>) -> Self {
        Self {
            tenant,
            name: name.into(),
        }
    }
}

/// Request-scoped tenant identifier.
///
/// App code may put it on the `Cx` with `cx.with(Tenant(id))`, and server code
/// and tests may carry it as a request extension; either wins over the signed-in
/// user's tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tenant(pub uuid::Uuid);

/// The tenant the request acts for, if any.
///
/// Checks a `Tenant` request extension first (a server-set override — the
/// deliberate seam app middleware and `Router::handle` tests use), then a
/// `Tenant` scoped value app code put on the `Cx`, then the signed-in user's
/// [`membership`]. No request header is consulted.
pub fn tenant_id(cx: &Cx) -> Option<uuid::Uuid> {
    if let Some(parts) = try_request_context::<http::request::Parts>(cx)
        && let Some(t) = parts.extensions.get::<Tenant>()
    {
        return Some(t.0);
    }
    if let Some(t) = try_request_context::<Tenant>(cx) {
        return Some(t.0);
    }
    selected(cx).map(|membership| membership.tenant)
}

/// The signed-in user's membership the request acts under: the tenant the
/// session selected while it is still one of the user's
/// [`tenants`](crate::auth::PanelUser::tenants), else the first. `None` without a
/// signed-in user, for a user with no tenant, and when a `Tenant` override
/// names a tenant the user is not a member of.
pub fn membership(cx: &Cx) -> Option<&Membership> {
    let tenant = tenant_id(cx)?;
    let signed = crate::auth::signed(cx)?;
    signed.user.tenants().iter().find(|m| m.tenant == tenant)
}

/// The session's selected membership, else the user's first.
fn selected(cx: &Cx) -> Option<&Membership> {
    let signed = crate::auth::signed(cx)?;
    let tenants = signed.user.tenants();
    signed
        .tenant
        .and_then(|tenant| tenants.iter().find(|m| m.tenant == tenant))
        .or_else(|| tenants.first())
}

/// Requires a tenant, returning an error if missing (for tenancy-gated resources).
///
/// # Errors
///
/// 403 when the request acts for no tenant.
pub fn require_tenant(cx: &Cx) -> Result<uuid::Uuid, topcoat::Error> {
    tenant_id(cx).ok_or_else(|| topcoat::router::error::forbidden().into())
}

/// How a resource's rows belong to a tenant.
///
/// [`Resource::tenancy`](crate::resource::Resource::tenancy) returns one. A
/// scoped tenancy names, by lens, the tenant UUID each row is filtered on:
///
/// ```text
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
    /// The model's own column, resolved when declared: its field, or `None`
    /// when the lens names none.
    Column {
        filter: TenantFilter,
        field: Option<TenantColumn>,
    },
    Via {
        filter: TenantFilter,
        /// Whether the lens is one field of the model: a `via` over its own
        /// column stamps nothing, so the mount refuses it in favor of
        /// [`Tenancy::column`](Self::column).
        single: bool,
        /// The model field the lens's first step names: the relation whose
        /// foreign key the form must write through a relationship field.
        hop: Option<usize>,
    },
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
    pub fn column<T>(lens: impl Into<Path<M, T>>) -> Self
    where
        T: Send + Sync + 'static,
        M: Send + Sync,
        uuid::Uuid: IntoExpr<T>,
    {
        let lens = lens.into();
        let field = crate::schema::lens_field(lens.clone(), &M::schema())
            .ok()
            .map(|field| TenantColumn {
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
    /// a row's tenant is its parent's. The lens starts at a `belongs_to`
    /// relation, and mounting the panel requires the form to write that
    /// relation's foreign key through a relationship field over a tenant-scoped
    /// resource, whose key the framework re-checks against that resource's
    /// tenant-scoped query inside the write.
    pub fn via<T>(lens: impl Into<Path<M, T>>) -> Self
    where
        T: Send + Sync + 'static,
        M: Send + Sync,
        uuid::Uuid: IntoExpr<T>,
    {
        let lens = lens.into();
        let hop = toasty_core::stmt::Path::from(lens.clone())
            .projection
            .as_slice()
            .first()
            .copied();
        let single = crate::schema::lens_field(lens.clone(), &M::schema()).is_ok();
        Self::scoped(Scope::Via {
            filter: Box::new(move |tenant| lens.clone().eq(tenant)),
            single,
            hop,
        })
    }

    /// Whether the rows belong to a tenant, so every handler requires one.
    pub fn is_scoped(&self) -> bool {
        !matches!(self.scope, Scope::None)
    }

    /// `<lens> = tenant`, or `None` for an unscoped resource.
    pub(crate) fn filter(&self, tenant: uuid::Uuid) -> Option<Expr<bool>> {
        match &self.scope {
            Scope::None => None,
            Scope::Column { filter, .. } | Scope::Via { filter, .. } => Some(filter(tenant)),
        }
    }

    /// Whether a [`via`](Self::via) tenancy names one field of the model,
    /// which [`Tenancy::column`](Self::column) owns. `None` for any other
    /// tenancy.
    pub(crate) fn via_is_single(&self) -> Option<bool> {
        match &self.scope {
            Scope::Via { single, .. } => Some(*single),
            Scope::None | Scope::Column { .. } => None,
        }
    }

    /// The model field a [`via`](Self::via) lens steps through first.
    pub(crate) fn via_hop(&self) -> Option<usize> {
        match &self.scope {
            Scope::Via { hop, .. } => *hop,
            Scope::None | Scope::Column { .. } => None,
        }
    }

    /// The model's own tenant column for a [`column`](Self::column) tenancy,
    /// or the mistake of a lens that names none.
    pub(crate) fn column_field(&self) -> Option<Result<&TenantColumn, DeclarationErrorKind>> {
        match &self.scope {
            Scope::Column { field, .. } => Some(
                field
                    .as_ref()
                    .ok_or(DeclarationErrorKind::TenancyColumnNotAField),
            ),
            Scope::None | Scope::Via { .. } => None,
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
