//! A table's ordering: the [default sort](Table::default_sort) it declares, and the sort and
//! order-by each request reads.

use toasty::stmt::{List, OrderBy, OrderByExpr, Path};

use super::{BoxColumn, Sort, Table, TableState};
use crate::schema::Binding;

impl<M> Table<M> {
    /// Sort by `path`'s column, smallest first, when the URL names no sortable column; by the
    /// first sortable column, ascending, by default.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// use tablo_core::{Table, TextColumn, lens};
    ///
    /// Table::new(TextColumn::new(lens!(Post.title)).sortable()).default_sort(lens!(Post.title));
    /// ```
    ///
    /// A path whose column the table does not declare sortable is a declaration error.
    pub fn default_sort<T>(self, path: impl Into<Path<M, T>>) -> Self
    where
        M: toasty::schema::Model,
    {
        self.sorted_by(path.into(), false)
    }

    /// Sort by `path`'s column, largest first, when the URL names no sortable column, as
    /// [`default_sort`](Self::default_sort) does ascending.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, published_at: jiff::Timestamp }
    /// use tablo_core::{DateColumn, Table, TextColumn, lens};
    ///
    /// // The newest posts first.
    /// Table::new((
    ///     TextColumn::new(lens!(Post.title)).sortable(),
    ///     DateColumn::new(lens!(Post.published_at)).sortable(),
    /// ))
    /// .default_sort_desc(lens!(Post.published_at));
    /// ```
    pub fn default_sort_desc<T>(self, path: impl Into<Path<M, T>>) -> Self
    where
        M: toasty::schema::Model,
    {
        self.sorted_by(path.into(), true)
    }

    fn sorted_by<T>(mut self, path: Path<M, T>, descending: bool) -> Self
    where
        M: toasty::schema::Model,
    {
        self.default_sort = Some(Sort {
            column: Binding::of(&path).name().to_string(),
            descending,
        });
        self
    }

    /// The sortable column named `name`.
    pub(super) fn sortable_column(&self, name: &str) -> Option<&BoxColumn<M>> {
        self.columns
            .iter()
            .find(|c| c.is_sortable() && c.name() == name)
    }

    /// The sort `state` reads: its own when it names a sortable column, else the
    /// [default](Self::default_sort), else the first sortable column's, ascending. `None` when no
    /// column sorts.
    pub(crate) fn sort_for(&self, state: &TableState) -> Option<Sort> {
        [state.sort.as_ref(), self.default_sort.as_ref()]
            .into_iter()
            .flatten()
            .find(|sort| self.sortable_column(&sort.column).is_some())
            .cloned()
            .or_else(|| {
                let first = self.columns.iter().find(|c| c.is_sortable())?;
                Some(Sort::asc(first.name()))
            })
    }

    /// The first sortable column's ascending order-by: the order a relationship's choices list in,
    /// whatever the list's [default](Self::default_sort).
    pub(crate) fn first_order_by(&self) -> Option<OrderByExpr>
    where
        M: toasty::schema::Model,
    {
        self.columns.iter().find_map(|c| c.order_by(false))
    }

    /// The order-by `sort` names.
    fn order_by(&self, sort: &Sort) -> Option<OrderByExpr>
    where
        M: toasty::schema::Model,
    {
        self.sortable_column(&sort.column)?
            .order_by(sort.descending)
    }

    /// Resolve the full query ordering for a request: its [sort](Self::sort_for), or the primary
    /// key when no column sorts.
    pub(crate) fn order_bys_for(&self, state: &TableState) -> Vec<OrderByExpr>
    where
        M: toasty::schema::Model,
    {
        match self.sort_for(state).and_then(|sort| self.order_by(&sort)) {
            Some(ord) => vec![ord],
            None => crate::toasty_compat::pk::pk_order_bys::<M>(),
        }
    }

    /// Order `query` as `state` sorts it, replacing any ordering `query` carries: Toasty appends
    /// an `order_by` after the existing ones, so a [`Resource::query`](crate::Resource::query)
    /// ordering would otherwise outrank the user's sort.
    pub(crate) fn order(
        &self,
        mut query: toasty::stmt::Query<List<M>>,
        state: &TableState,
    ) -> toasty::stmt::Query<List<M>>
    where
        M: toasty::schema::Model,
    {
        query.set_order_by(OrderBy {
            exprs: self.order_bys_for(state),
        });
        query
    }

    /// The ordering an offset page reads: `state`'s sort, then the primary key.
    ///
    /// Toasty breaks ties by the key for a cursor only; without it, rows sharing a sort value may
    /// come back in a different order for each offset, and repeat or go missing across pages.
    pub(crate) fn offset_order_bys_for(&self, state: &TableState) -> Vec<OrderByExpr>
    where
        M: toasty::schema::Model,
    {
        let mut order = self.order_bys_for(state);
        let key = crate::toasty_compat::pk::pk_order_bys::<M>();
        if order != key {
            order.extend(key);
        }
        order
    }
}

#[cfg(test)]
mod tests;
