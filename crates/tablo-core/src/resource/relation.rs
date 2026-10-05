//! Relation tables: a record page's related rows through the related resource's own list table.

use std::{any::TypeId, sync::Arc};

use toasty::stmt::{Expr, IntoExpr, Path};

use super::Resource;
use crate::{form::FormScalar, schema::ResolvedLens, toasty_compat::pk};

/// One relation of a parent resource's records.
///
/// Declared by [`ResourceDef::relation`](super::ResourceDef::relation); the related resource
/// must be registered on the same panel, which keys the relation by its slug:
///
/// ```text
/// ResourceDef::new().relation(Relation::has_many::<CommentResource>(Comment::fields().post_id()))
/// ```
pub struct Relation<P> {
    pub(crate) child: TypeId,
    pub(crate) child_name: &'static str,
    pub(crate) label: Option<String>,
    pub(crate) foreign_key: String,
    bind: BindFn<P>,
    pub(crate) scope_of: ScopeFn,
    /// Why `foreign_key` binds no column, when it does not.
    pub(crate) misdeclared: Option<crate::DeclarationErrorKind>,
}

/// An owner's rows filter on the child.
type BindFn<P> = Arc<dyn Fn(&P) -> (Expr<bool>, String) + Send + Sync>;

/// The child's rows that belong to the owner whose primary key a request names as text.
pub(crate) type ScopeFn = Arc<dyn Fn(&str) -> Option<Expr<bool>> + Send + Sync>;

/// A foreign-key column type referencing a primary key of type `K`: `K` itself, or `Option<K>`
/// for an optional reference. Sealed: these are the only two.
pub trait ForeignKey<K>:
    sealed::Sealed<K> + IntoExpr<Self> + FormScalar + Send + Sync + 'static
{
}

impl<K> ForeignKey<K> for K where K: IntoExpr<K> + FormScalar + Send + Sync + 'static {}

impl<K> ForeignKey<K> for Option<K> where
    Option<K>: IntoExpr<Self> + FormScalar + Send + Sync + 'static
{
}

mod sealed {
    pub trait Sealed<K> {}

    impl<K> Sealed<K> for K {}

    impl<K> Sealed<K> for Option<K> {}
}

impl<P> Relation<P>
where
    P: toasty::schema::Model + IntoExpr<P> + 'static,
{
    /// Declare a `has_many` child resource whose `foreign_key` holds the owner's primary key, a
    /// single column.
    ///
    /// ```text
    /// Relation::has_many::<CommentResource>(Comment::fields().post_id())
    /// ```
    pub fn has_many<C>(foreign_key: Path<C::Model, impl ForeignKey<P::PrimaryKey>>) -> Self
    where
        C: Resource,
    {
        Self::bound_by::<C, _>(foreign_key)
    }

    fn bound_by<C, T>(foreign_key: Path<C::Model, T>) -> Self
    where
        C: Resource,
        T: ForeignKey<P::PrimaryKey>,
    {
        let binding = ResolvedLens::of(foreign_key.clone());
        let scope_key = foreign_key.clone();
        Self {
            child: TypeId::of::<C>(),
            child_name: std::any::type_name::<C>(),
            label: None,
            foreign_key: binding.name,
            misdeclared: binding.misdeclared,
            bind: Arc::new(move |owner| {
                let scope = foreign_key.clone().eq(pk::pk_expr::<P, T>(owner));
                (scope, pk::pk_text(owner))
            }),
            scope_of: Arc::new(move |owner| {
                let owner = T::parse_form(owner.trim()).ok()?;
                Some(scope_key.clone().eq(owner))
            }),
        }
    }
}

impl<P> Relation<P> {
    /// Titles the section `label` instead of the related resource's plural label.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The child's rows that belong to `owner`, and the owner's primary key as text.
    pub(crate) fn bind(&self, owner: &P) -> (Expr<bool>, String) {
        (self.bind)(owner)
    }
}

impl<P> std::fmt::Debug for Relation<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relation")
            .field("child", &self.child_name)
            .field("label", &self.label)
            .field("foreign_key", &self.foreign_key)
            .finish_non_exhaustive()
    }
}
