//! The tenant column type.

use toasty::{
    codegen_support::index::IndexableField,
    schema::{Field, Load},
    stmt::{Assign, Assignment, Expr, IntoExpr, List, Path},
};
use toasty_core::{
    Error,
    stmt::{self, Assignments, Projection, Value},
};

/// One tenant's id, and the field type of a resource's tenant column.
///
/// A model declares its tenant column with this type:
///
/// ```
/// # use tablo_core::TenantId;
/// #[derive(Debug, Clone, toasty::Model)]
/// pub struct Note {
///     #[key]
///     #[auto]
///     id: uuid::Uuid,
///     #[index]
///     tenant_id: TenantId,
///     body: String,
/// }
/// ```
///
/// [`Tenancy::column`](crate::Tenancy::column) and
/// [`Tenancy::via`](crate::Tenancy::via) accept only a [`TenantColumn`], so a lens naming any
/// other `Uuid` column — an author, a parent row, the key — does not compile. The column stores a
/// UUID, so the database column is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TenantId(uuid::Uuid);

impl TenantId {
    /// The id, as [`tenant_id`](crate::tenant_id) answers it and a session carries it.
    pub fn get(self) -> uuid::Uuid {
        self.0
    }
}

impl From<uuid::Uuid> for TenantId {
    fn from(id: uuid::Uuid) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for TenantId {
    type Err = uuid::Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        uuid::Uuid::from_str(text).map(Self)
    }
}

impl Load for TenantId {
    type Output = Self;

    fn ty() -> stmt::Type {
        stmt::Type::Uuid
    }

    fn load(value: stmt::Value) -> Result<Self::Output, Error> {
        match value {
            stmt::Value::Uuid(id) => Ok(Self(id)),
            _ => Err(Error::type_conversion(value, "TenantId")),
        }
    }

    fn reload(target: &mut Self::Output, value: stmt::Value) -> Result<(), Error> {
        *target = Self::load(value)?;
        Ok(())
    }
}

impl Field for TenantId {
    type ExprTarget = Self;
    type Path<Origin> = Path<Origin, Self>;
    type ListPath<Origin> = Path<Origin, List<Self::ExprTarget>>;
    type Update<'a> = ();
    type Inner = Self;

    fn new_path<Origin>(path: Path<Origin, Self>) -> Self::Path<Origin> {
        path
    }

    fn new_list_path<Origin>(path: Path<Origin, List<Self::ExprTarget>>) -> Self::ListPath<Origin> {
        path
    }

    fn new_update<'a>(
        _assignments: &'a mut Assignments,
        _projection: Projection,
    ) -> Self::Update<'a> {
    }

    fn key_constraint<Origin>(&self, target: Path<Origin, Self::Inner>) -> Expr<bool> {
        target.eq(*self)
    }
}

impl IndexableField for TenantId {}

impl IntoExpr<TenantId> for TenantId {
    fn into_expr(self) -> Expr<TenantId> {
        tenant_expr(self.0)
    }

    fn by_ref(&self) -> Expr<TenantId> {
        tenant_expr(self.0)
    }
}

impl Assign<TenantId> for TenantId {
    fn into_assignment(self) -> Assignment<TenantId> {
        toasty::stmt::set(IntoExpr::<TenantId>::into_expr(self))
    }
}

fn tenant_expr(id: uuid::Uuid) -> Expr<TenantId> {
    Expr::from_untyped(stmt::Expr::Value(Value::Uuid(id)))
}

/// A field type a tenancy names: [`TenantId`], or `Option<TenantId>` for a
/// nullable column.
///
/// Implement it for an app's own newtype to name that instead. `uuid::Uuid` cannot implement it:
/// neither the trait nor the type belongs to the app, so an unmarked UUID column can never be
/// named as a tenant column.
///
/// ```compile_fail,E0277
/// # use tablo_core::{Tenancy, TenantId};
/// #[derive(Debug, Clone, toasty::Model)]
/// struct Post {
///     #[key]
///     #[auto]
///     id: uuid::Uuid,
///     #[index]
///     tenant_id: TenantId,
///     author_id: uuid::Uuid,
/// }
///
/// // `author_id` is a `Uuid`: the lens names no tenant column.
/// let _ = Tenancy::column(Post::fields().author_id());
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a tenant column",
    label = "not a tenant column",
    note = "type the model's tenant field as `TenantId`, or `Option<TenantId>` for a nullable one"
)]
pub trait TenantColumn: Sized + Field + IntoExpr<Self> + Send + Sync + 'static {
    /// The column's value for the request tenant `id`.
    fn from_tenant(id: uuid::Uuid) -> Self;
}

impl TenantColumn for TenantId {
    fn from_tenant(id: uuid::Uuid) -> Self {
        Self(id)
    }
}

impl TenantColumn for Option<TenantId> {
    fn from_tenant(id: uuid::Uuid) -> Self {
        Some(TenantId(id))
    }
}
