//! Relation tables: a record page's related rows through the related resource's own list table.

use std::sync::Arc;

use toasty::stmt::{Expr, IntoExpr, Path};
use topcoat::{context::Cx, view::BoxView};

use super::Resource;
use crate::{form::FormScalar, schema::ResolvedLens, toasty_compat::pk};

/// One relation of a parent resource's records.
///
/// Declared by [`Resource::relations`]:
///
/// ```ignore
/// fn relations() -> Vec<Relation<Post>> {
///     vec![Relation::has_many::<CommentResource>(Comment::fields().post_id())]
/// }
/// ```
pub struct Relation<P> {
    key: String,
    label: String,
    foreign_key: String,
    bind: BindFn<P>,
    render: RenderFn,
    search: crate::panel::RelationSearchFn,
    /// Why `foreign_key` binds no column, when it does not.
    misdeclared: Option<String>,
}

/// An owner's rows filter on the child.
type BindFn<P> = Arc<dyn Fn(&P) -> (Expr<bool>, String) + Send + Sync>;

/// The monomorphized renderer of one child resource.
pub(crate) type RenderFn = for<'a> fn(&'a Cx, BoundRelation) -> BoxView<'a>;

/// A relation resolved against one owner record.
pub(crate) struct BoundRelation {
    /// The slug of the resource that owns the record.
    pub(crate) parent: String,
    /// The prefix of the table's URL parameters.
    pub(crate) key: String,
    /// The section title.
    pub(crate) label: String,
    /// The child's rows that belong to the owner.
    pub(crate) scope: Expr<bool>,
    /// The child's form key for the owner, and the owner's value for it.
    pub(crate) seed: (String, String),
    /// The path of the page that renders the table.
    pub(crate) page: String,
    /// Whether the page only shows the rows.
    pub(crate) read_only: bool,
}

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
    /// ```ignore
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
        let search = crate::panel::relation_search_handler_for::<C, T>(foreign_key.clone());
        Self {
            key: C::slug(),
            label: C::navigation_label(),
            foreign_key: binding.name,
            misdeclared: binding.misdeclared,
            bind: Arc::new(move |owner| {
                let scope = foreign_key.clone().eq(pk::pk_expr::<P, T>(owner));
                (scope, pk::pk_text(owner))
            }),
            render: crate::panel::relation_table::<C>,
            search,
        }
    }
}

impl<P> Relation<P> {
    /// Title the section `label`.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Why the relation's foreign key binds no column.
    pub(crate) fn misdeclared(&self) -> Option<&str> {
        self.misdeclared.as_deref()
    }

    /// The prefix of this relation's URL parameters.
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    /// The live-search loader of this relation's table.
    pub(crate) fn search_handler(&self) -> crate::panel::RelationSearchFn {
        self.search.clone()
    }

    /// Render this relation's table for `owner` on the page at `page`.
    pub(crate) fn render<'a>(
        &self,
        cx: &'a Cx,
        owner: &P,
        page: &str,
        read_only: bool,
        parent: &str,
    ) -> BoxView<'a> {
        let (scope, value) = (self.bind)(owner);
        (self.render)(
            cx,
            BoundRelation {
                parent: parent.to_string(),
                key: self.key.clone(),
                label: self.label.clone(),
                scope,
                seed: (self.foreign_key.clone(), value),
                page: page.to_string(),
                read_only,
            },
        )
    }
}

impl<P> std::fmt::Debug for Relation<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relation")
            .field("key", &self.key)
            .field("label", &self.label)
            .field("foreign_key", &self.foreign_key)
            .finish_non_exhaustive()
    }
}
