//! [`ResourceDef::query`]: a resource's own row scoping within its panel.

use std::sync::Arc;

use toasty::stmt::Expr;
use topcoat::context::Cx;

use super::{Resource, ResourceDef};

/// One [`ResourceDef::query`] narrowing: the predicate its rows match in a request, or `None`
/// when it narrows nothing there.
pub(crate) type QueryFn = Arc<dyn Fn(&Cx) -> Option<Expr<bool>> + Send + Sync>;

impl<R: Resource> ResourceDef<R> {
    /// Narrows the rows the resource serves within the panel to the ones `predicate` matches: its
    /// own row scoping, such as row visibility. Every loader reads through it: the list and its
    /// counts, the export, the detail, edit and record pages, every write, relationship options
    /// and relation tables, the global search, widgets and the sidebar badge. A row it leaves out
    /// does not exist for the request: its pages answer 404.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, published: bool }
    /// # struct PostResource;
    /// # impl tablo_core::Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = tablo_core::NoForm<Post>;
    /// # }
    /// use tablo_core::Panel;
    ///
    /// // The portal lists and opens published posts only.
    /// Panel::new("portal")
    ///     .resource_with::<PostResource>(|def| def.query(Post::fields().published().eq(true)));
    /// ```
    ///
    /// [`query_fn`](Self::query_fn) narrows by the request, such as to the signed-in user's rows.
    ///
    /// Each call ANDs its predicate onto the ones before it, so a panel's
    /// [`resource_with`](crate::Panel::resource_with) narrows what the resource's own
    /// [`declare`](Resource::declare) narrows, and none widens another.
    ///
    /// **Tenancy and soft deletes are not this method's job.** The framework ANDs the
    /// [`tenancy`](Self::tenancy) filter and leaves out the [trashed](Self::soft_delete) records
    /// beside every narrowing. Code outside the framework's loaders starts from
    /// [`scoped_query`](crate::scoped_query), which reads through the same narrowings.
    ///
    /// **Neither are relations, an order or a limit.** A predicate says which rows, and nothing
    /// else: the list, the export and the detail page load the relations their columns declare
    /// ([`Column::includes`](crate::extend::Column::includes)),
    /// [`include_unscoped`](Self::include_unscoped) loads one a closure no column covers reads,
    /// and [`Table::default_sort`](crate::Table::default_sort) orders a list.
    ///
    /// # Unique constraints and the narrowed rows
    ///
    /// The app-side unique pre-check reads each narrowing's predicate. One that reads a field of
    /// the unique index stays in the probe, as it fixes that field: under
    /// `query_fn(|cx| owner().eq(me(cx)))` a `#[unique(owner, slug)]` slug is taken only among the
    /// user's own rows. One that reads none of the index's fields is left out of the probe, since
    /// the index covers the rows it hides: a slug an unpublished post holds is refused inline as
    /// "has already been taken". A predicate over the index's fields that still hides a
    /// colliding row, such as `owner().ne(..)`, lets the database refuse the write; probe that
    /// index in [`create_record`](Resource::create_record) and
    /// [`update_record`](Resource::update_record) and refuse with
    /// [`WriteError::Refused`](crate::WriteError::Refused).
    #[must_use]
    pub fn query(mut self, predicate: Expr<bool>) -> Self {
        self.queries
            .push(Arc::new(move |_cx| Some(predicate.clone())));
        self
    }

    /// Narrows the rows as [`query`](Self::query) does, by the request: to the ones the predicate
    /// it returns matches, or not at all when it returns `None`.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, author: String }
    /// # struct PostResource;
    /// # impl tablo_core::Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = tablo_core::NoForm<Post>;
    /// # }
    /// # fn current_user(_cx: &topcoat::context::Cx) -> String { String::new() }
    /// # fn is_editor(_cx: &topcoat::context::Cx) -> bool { false }
    /// use tablo_core::Panel;
    ///
    /// Panel::new("portal").resource_with::<PostResource>(|def| {
    ///     // Each author lists and opens their own posts only.
    ///     def.query_fn(|cx| Post::fields().author().eq(current_user(cx)))
    /// });
    ///
    /// Panel::new("desk").resource_with::<PostResource>(|def| {
    ///     // An editor reads every post.
    ///     def.query_fn(|cx| (!is_editor(cx)).then(|| Post::fields().author().eq(current_user(cx))))
    /// });
    /// ```
    ///
    /// The closure runs once per query a request makes, reads the request alone and cannot fail:
    /// the [`policy`](Self::policy) refuses a request that may read no row. A predicate over
    /// rows the request must first read, the teams a user sits on say, reads them from a
    /// [`Load`](crate::Load) value the panel loaded.
    #[must_use]
    pub fn query_fn<F, P>(mut self, predicate: F) -> Self
    where
        F: Fn(&Cx) -> P + Send + Sync + 'static,
        P: Into<Option<Expr<bool>>>,
    {
        self.queries.push(Arc::new(move |cx| predicate(cx).into()));
        self
    }

    /// Loads `relation` with every row the resource reads, for a closure no column covers: one
    /// the [`policy`](Self::policy) reads, or one a table's `group_by` or row key reads without a
    /// column including it. Call it once per relation.
    ///
    /// The closure reads whatever the include loads, under no resource's query or policy, as an
    /// [`unscoped`](crate::RelationColumn::unscoped) relation column does. Every loader pays for
    /// the relation, so one only a column shows belongs to that column.
    #[must_use]
    pub fn include_unscoped<T>(
        mut self,
        relation: impl Into<toasty::stmt::Include<R::Model, T>>,
    ) -> Self {
        self.includes = self.includes.with(relation);
        self
    }
}
