//! [`ResourceDef::query`]: a resource's own row scoping within its panel.

use std::sync::Arc;

use toasty::stmt::{List, Query};
use topcoat::context::Cx;

use super::{Resource, ResourceDef};

/// One [`ResourceDef::query`] narrowing, applied to the query the ones before it returned.
pub(crate) type QueryFn<M> = Arc<dyn Fn(&Cx, Query<List<M>>) -> Query<List<M>> + Send + Sync>;

impl<R: Resource> ResourceDef<R> {
    /// Narrows the rows the resource serves within the panel: its own row scoping, such as row
    /// visibility. Every loader reads through it: the list and its counts, the export, the
    /// detail, edit and record pages, every write, relationship options and relation tables, the
    /// global search, widgets and the sidebar badge. A row it leaves out does not exist for the
    /// request: its pages answer 404.
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
    /// use tablo_core::Panel;
    ///
    /// // Each author lists and opens their own posts only.
    /// Panel::new("portal").resource_with::<PostResource>(|def| {
    ///     def.query(|cx, query| query.filter(Post::fields().author().eq(current_user(cx))))
    /// });
    /// ```
    ///
    /// Each call receives the query the calls before it return, so a panel's
    /// [`resource_with`](crate::Panel::resource_with) narrows what the resource's own
    /// [`declare`](Resource::declare) narrows.
    ///
    /// **Tenancy and soft deletes are not this method's job.** The framework ANDs the
    /// [`tenancy`](Self::tenancy) filter and leaves out the [trashed](Self::soft_delete) records
    /// after every narrowing, so none can drop them. Code outside the framework's loaders starts
    /// from [`scoped_query`](crate::scoped_query), which reads through the same narrowings.
    ///
    /// **Relations are not its job either.** The list, the export and the detail page load the
    /// relations their columns declare ([`Column::includes`](crate::extend::Column::includes)):
    /// a [`RelationColumn`](crate::RelationColumn) declares its own. Include a relation here
    /// only when a closure no column covers reads it on every loader's rows: one the
    /// [`policy`](Self::policy) reads, or one a table's `group_by` or row key reads without a
    /// column including it.
    ///
    /// A list and its export replace any ordering a narrowing sets with the table's own sort:
    /// order a list with [`Table::default_sort`](crate::Table::default_sort).
    ///
    /// # Keep unique constraints in step with the narrowed rows
    ///
    /// The app-side unique pre-check probes through the same query, so a `#[unique]` index
    /// *broader* than it is invisible: the probe misses the colliding row and the user gets a 500
    /// instead of the inline "has already been taken". Scope the constraint to match —
    /// `#[unique(tenant_id, email)]` — or, when the index must stay broader, probe it in
    /// [`create_record`](Resource::create_record) and [`update_record`](Resource::update_record)
    /// and refuse with [`WriteError::Refused`](crate::WriteError::Refused). Not checkable at
    /// declaration time, since a query's filters are not introspectable; tablo#117 tracks the fix.
    #[must_use]
    pub fn query<F>(mut self, query: F) -> Self
    where
        F: Fn(&Cx, Query<List<R::Model>>) -> Query<List<R::Model>> + Send + Sync + 'static,
    {
        self.queries.push(Arc::new(query));
        self
    }
}
