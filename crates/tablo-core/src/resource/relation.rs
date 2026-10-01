//! Relation tables: a record page's related rows, through the related
//! resource's own list table.
//!
//! A [`Relation`] names a child [`Resource`] and the key that ties its rows to
//! the owner record. The panel renders it on the owner's detail and edit pages
//! as the child's list table — its columns, search, sort, filters, pager and
//! row actions — with the rows narrowed to the owner. The same binding seeds
//! the child's create form, so a related record is created from the page that
//! shows it.

use std::sync::Arc;

use toasty::stmt::{Expr, IntoExpr};
use topcoat::{context::Cx, view::BoxView};

use super::Resource;
use crate::{
    form::FormScalar,
    schema::{FieldLens, LensBinding},
};

/// One relation of a parent resource's records: the child resource whose rows
/// belong to a record, and the key that says which.
///
/// Declared by [`Resource::relations`]:
///
/// ```ignore
/// fn relations() -> Vec<Relation<Post>> {
///     vec![Relation::has_many::<CommentResource, _>(
///         Comment::fields().post_id(),
///         |post: &Post| post.id,
///     )]
/// }
/// ```
///
/// Every row goes through the child's own tenant-scoped query and policies —
/// `can_view_any` decides whether the table renders at all, and the row
/// actions are gated per row as on its list. Writes started from the table
/// return to the page that shows it.
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

/// An owner's rows filter on the child, and the owner's key as the child's
/// form value.
type BindFn<P> = Arc<dyn Fn(&P) -> (Expr<bool>, String) + Send + Sync>;

/// The monomorphized renderer of one child resource, over what a relation
/// resolved against its owner.
pub(crate) type RenderFn = for<'a> fn(&'a Cx, BoundRelation) -> BoxView<'a>;

/// A relation resolved against one owner record, for the page at `page`.
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
    /// Whether the page only shows the rows — the detail page — so the table
    /// carries no write action and no create link.
    pub(crate) read_only: bool,
}

impl<P> Relation<P> {
    /// The child resource `C`, whose `foreign_key` column holds the owner's
    /// `owner_key` — a post's comments:
    ///
    /// ```ignore
    /// Relation::has_many::<CommentResource, _>(Comment::fields().post_id(), |post: &Post| post.id)
    /// ```
    ///
    /// A nullable foreign key binds the same way, with the owner's key in
    /// `Some`. The key's form spelling ([`FormScalar::to_form`]) seeds `C`'s
    /// create form.
    ///
    /// The section is titled with `C`'s navigation label, and its table's URL
    /// parameters are prefixed with `C`'s slug (`comments.q=`).
    ///
    /// A `foreign_key` that is not a single column of `C`'s model is a
    /// misdeclaration [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel)
    /// reports, like every lens a declaration binds.
    pub fn has_many<C, T>(
        foreign_key: FieldLens<C::Model, T>,
        owner_key: impl Fn(&P) -> T + Send + Sync + 'static,
    ) -> Self
    where
        C: Resource,
        T: IntoExpr<T> + FormScalar + Send + Sync + 'static,
    {
        let binding = LensBinding::of(foreign_key.clone());
        let search = crate::panel::relation_search_handler_for::<C, T>(foreign_key.clone());
        Self {
            key: C::slug(),
            label: C::navigation_label(),
            foreign_key: binding.name,
            misdeclared: binding.misdeclared,
            bind: Arc::new(move |owner| {
                let value = owner_key(owner);
                let seed = value.to_form();
                (foreign_key.clone().eq(value), seed)
            }),
            render: crate::panel::relation_table::<C>,
            search,
        }
    }

    /// Title the section `label` instead of the child's navigation label.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Why the relation's foreign key binds no column, when it does not.
    pub(crate) fn misdeclared(&self) -> Option<&str> {
        self.misdeclared.as_deref()
    }

    /// The prefix of this relation's URL parameters: the child's slug.
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    /// The live-search loader of this relation's table, for the panel's
    /// relation registry.
    pub(crate) fn search_handler(&self) -> crate::panel::RelationSearchFn {
        self.search.clone()
    }

    /// Render this relation's table for `owner` on the page at `page`,
    /// without write actions when `read_only`. `parent` is the slug of the
    /// resource that owns the record, keying the relation's live-search
    /// handler alongside `key`.
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
