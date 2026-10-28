//! A table's ordering: the [default sort](Table::default_sort) it declares, and the sort and
//! order-by each request reads.

use toasty::stmt::{List, OrderBy, OrderByExpr, Path};

use super::{BoxColumn, Sort, Table, TableState};
use crate::schema::Binding;

/// The field a [`default_sort`](Table::default_sort) orders by, and its order-by in the
/// default's direction.
#[derive(Clone)]
pub(super) struct FieldOrder {
    binding: Binding,
    order: OrderByExpr,
}

impl FieldOrder {
    /// What the order reads of a record.
    pub(super) fn reads<M>(&self) -> crate::lens::Reads<M> {
        self.binding.reads::<M>()
    }
}

impl<M> Table<M> {
    /// Sort by the field `path` names, smallest first, when the URL names no sortable column; by
    /// the first sortable column, ascending, by default. The field needs no column: the rows
    /// sort by it all the same, and no header marks the sort.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// use tablo_core::{Table, TextColumn, lens};
    ///
    /// Table::new(TextColumn::new(lens!(Post.title)).sortable()).default_sort(lens!(Post.title));
    /// ```
    pub fn default_sort<T>(self, path: impl Into<Path<M, T>>) -> Self
    where
        M: toasty::schema::Model,
    {
        self.sorted_by(path.into(), false)
    }

    /// Sort by the field `path` names, largest first, when the URL names no sortable column, as
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

    /// Sort by the sortable column `sort` names when the URL names none: the default for a
    /// column of your own, whose [`name`](crate::extend::Column::name) no field path spells.
    /// A field's column takes [`default_sort`](Self::default_sort), whose path a rename fails
    /// to compile.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// use tablo_core::{Sort, Table, TextColumn, lens};
    ///
    /// // "revenue" is the name a column of your own returns.
    /// Table::new(TextColumn::new(lens!(Post.title))).default_sort_by(Sort::desc("revenue"));
    /// ```
    ///
    /// A name the table declares no sortable column for is a declaration error.
    pub fn default_sort_by(mut self, sort: Sort) -> Self {
        self.default_sort = Some(sort);
        self.default_field = None;
        self
    }

    fn sorted_by<T>(mut self, path: Path<M, T>, descending: bool) -> Self
    where
        M: toasty::schema::Model,
    {
        let binding = Binding::of(&path);
        // A path that names no field reports that, not a column named after its raw steps.
        if let Some(error) = binding.misdeclared() {
            self.misdeclared.push(error);
            return self;
        }
        self.default_sort = Some(Sort {
            column: binding.name().to_string(),
            descending,
        });
        let order = if descending { path.desc() } else { path.asc() };
        self.default_field = Some(FieldOrder { binding, order });
        self
    }

    /// Whether `sort` orders the rows: it names a sortable column, or it is the default sort on
    /// a field.
    pub(super) fn orders_by(&self, sort: &Sort) -> bool {
        self.sortable_column(&sort.column).is_some() || self.field_order(sort).is_some()
    }

    /// The order-by of the default sort on a field, when `sort` is that default.
    fn field_order(&self, sort: &Sort) -> Option<&OrderByExpr> {
        let field = self.default_field.as_ref()?;
        (self.default_sort.as_ref() == Some(sort) && field.binding.name() == sort.column)
            .then_some(&field.order)
    }

    /// The sortable column named `name`.
    pub(super) fn sortable_column(&self, name: &str) -> Option<&BoxColumn<M>> {
        self.columns
            .iter()
            .find(|c| c.is_sortable() && c.name() == name)
    }

    /// The sort `state` reads: its own when it names a sortable column, else the
    /// [default](Self::default_sort), else the first sortable column's, ascending. `None` when
    /// nothing sorts.
    pub(crate) fn sort_for(&self, state: &TableState) -> Option<Sort> {
        state
            .sort
            .as_ref()
            .filter(|sort| self.sortable_column(&sort.column).is_some())
            .or_else(|| {
                self.default_sort
                    .as_ref()
                    .filter(|sort| self.orders_by(sort))
            })
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
        match self.sortable_column(&sort.column) {
            Some(column) => column.order_by(sort.descending),
            None => self.field_order(sort).cloned(),
        }
    }

    /// Resolve the full query ordering for a request: its [grouping](Self::group_by)'s field,
    /// unless the sort is on it, then its [sort](Self::sort_for), or the primary key when nothing
    /// sorts.
    pub(crate) fn order_bys_for(&self, state: &TableState) -> Vec<OrderByExpr>
    where
        M: toasty::schema::Model,
    {
        let sort = self.sort_for(state);
        let mut order: Vec<OrderByExpr> = self
            .active_grouping(state)
            .filter(|group| sort.as_ref().is_none_or(|sort| sort.column != group.name()))
            .map(|group| group.order().clone())
            .into_iter()
            .collect();
        match sort.and_then(|sort| self.order_by(&sort)) {
            Some(ord) => order.push(ord),
            None => order.extend(crate::toasty_compat::pk::pk_order_bys::<M>()),
        }
        order
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
        for key in crate::toasty_compat::pk::pk_order_bys::<M>() {
            if !order.contains(&key) {
                order.push(key);
            }
        }
        order
    }
}

#[cfg(test)]
mod tests;
