//! A resource's list view: the [`Table`] builder and its columns and filters, the URL state a list
//! reads, query planning (`filter_expr`/`search_expr`/`order_bys_for`), and the page a table loads.
//!
//! What a request wires onto a table (row actions, policy) lives in `wiring`, rendering
//! in `render`, CSV export in `export`.

use std::{marker::PhantomData, num::NonZeroUsize, sync::Arc};

use toasty::stmt::{Expr, List, OrderByExpr};

use self::{column::BoxColumn, filter::BoxFilter};
use crate::{
    DeclarationErrorKind, Lens,
    form::FormScalar,
    schema::{Binding, FieldResolver},
};

mod column;
mod export;
mod filter;
mod page;
mod render;
mod state;
mod wiring;

pub use self::{
    column::{
        BooleanColumn, Column, ColumnWidth, ComputedColumn, Includes, IntoColumns, TextColumn,
        contains_expr,
    },
    filter::{
        DateFilter, Filter, FilterInput, IntoFilters, QueryFilter, SelectFilter, TernaryFilter,
    },
    page::TablePage,
    state::{Cursor, Sort, TableState},
    wiring::WiredTable,
};
pub(crate) use self::{
    page::{Past, row_exists_past},
    state::{
        ACTION_ROUTE_PARAM, ACTIONS_ROUTE_SEGMENT, BULK_DELETE_ROUTE_SEGMENT, CREATE_ROUTE_SEGMENT,
        DASH_ROUTE_SEGMENT, DELETE_ROUTE_SEGMENT, EDIT_ROUTE_SEGMENT, RECORD_ROUTE_PARAM,
        RETURN_PARAM, create_page_url, with_return,
    },
};

/// Group-label projection reads a row's group off one model instance.
pub(crate) type GroupKey<M> = Arc<dyn Fn(&M) -> String + Send + Sync>;

/// Per-record action policy reads which row actions one model instance allows.
pub(crate) type RowPolicy<M> = Arc<dyn Fn(&M) -> RowActions + Send + Sync>;

/// Which row actions one record may use.
///
/// A denied action emits no link, and a row denied `delete` renders no bulk checkbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RowActions {
    /// Whether the row renders its `View` link.
    pub view: bool,
    /// Whether the row renders its `Edit` link.
    pub edit: bool,
    /// Whether the row renders its `Delete` link and an enabled bulk checkbox.
    pub delete: bool,
}

impl RowActions {
    /// Every action allowed.
    pub const ALL: Self = Self {
        view: true,
        edit: true,
        delete: true,
    };
}

/// A named grouping a `Table` renders.
pub(crate) struct GroupDef<M> {
    binding: Binding,
    key: GroupKey<M>,
}

impl<M> Clone for GroupDef<M> {
    fn clone(&self) -> Self {
        Self {
            binding: self.binding.clone(),
            key: Arc::clone(&self.key),
        }
    }
}

/// The action chrome a resource declares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TableChrome {
    /// Whether the row renders a Delete action (which also enables bulk).
    pub(crate) delete: bool,
    /// Whether the row renders an Edit action.
    pub(crate) edit: bool,
    /// Whether the row renders a View action.
    pub(crate) view: bool,
    /// Whether the table renders the resource's custom actions.
    pub(crate) actions: bool,
}

/// One custom [`Action`](crate::resource::Action) as a table renders it.
pub(crate) struct TableAction<M> {
    pub(crate) name: &'static str,
    pub(crate) label: String,
    pub(crate) row: bool,
    pub(crate) bulk: bool,
    pub(crate) confirm: bool,
    pub(crate) allowed: Arc<dyn Fn(&M) -> bool + Send + Sync>,
}

/// The page size of a table that declares none with [`Table::paginate`].
pub const DEFAULT_PAGE_SIZE: NonZeroUsize = NonZeroUsize::new(25).unwrap();

/// Table description of a `Resource`'s list view declaring columns and how they map to queries.
pub struct Table<M> {
    columns: Vec<BoxColumn<M>>,
    /// Misdeclarations a builder recorded ([`Self::declaration_errors`]).
    misdeclared: Vec<DeclarationErrorKind>,
    filters: Vec<BoxFilter<M>>,
    group_by: Option<GroupDef<M>>,
    /// A row's key: its record's primary key, as its action URLs carry it.
    key: fn(&M) -> String,
    /// Whether a row's key resolves as a URL id; a composite key does not, so its rows carry no
    /// action.
    addressable: bool,
    page_size: NonZeroUsize,
    hide_search: bool,
    hide_filter_bar: bool,
    _marker: PhantomData<M>,
}

/// A copy sharing every column, filter and projection.
impl<M> Clone for Table<M> {
    fn clone(&self) -> Self {
        Self {
            columns: self.columns.clone(),
            misdeclared: self.misdeclared.clone(),
            filters: self.filters.clone(),
            group_by: self.group_by.clone(),
            key: self.key,
            addressable: self.addressable,
            page_size: self.page_size,
            hide_search: self.hide_search,
            hide_filter_bar: self.hide_filter_bar,
            _marker: PhantomData,
        }
    }
}

impl<M> std::fmt::Debug for Table<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Table")
            .field(
                "columns",
                &self.columns.iter().map(|c| c.name()).collect::<Vec<_>>(),
            )
            .field("filters", &self.filters.len())
            .field("group_by", &self.group_by.is_some())
            .field("page_size", &self.page_size)
            .field("hide_search", &self.hide_search)
            .field("hide_filter_bar", &self.hide_filter_bar)
            .finish()
    }
}

impl<M> Table<M> {
    /// Declare a table of `cols`.
    ///
    /// Each row is keyed by its record's primary key, which the row's action URLs carry.
    pub fn new(cols: impl IntoColumns<M>) -> Self
    where
        M: toasty::schema::Model + toasty::stmt::IntoExpr<M>,
    {
        Self {
            columns: cols.into_columns(),
            misdeclared: Vec::new(),
            filters: Vec::new(),
            group_by: None,
            key: crate::toasty_compat::pk::pk_text::<M>,
            addressable: !crate::toasty_compat::pk::pk_is_composite::<M>(),
            page_size: DEFAULT_PAGE_SIZE,
            hide_search: false,
            hide_filter_bar: false,
            _marker: PhantomData,
        }
    }

    /// Append `column` after the declared ones.
    pub fn column(mut self, column: impl Column<M> + 'static) -> Self {
        self.columns.push(Arc::new(column));
        self
    }

    /// Declare filters.
    pub fn filters(mut self, filters: impl IntoFilters<M>) -> Self
    where
        M: toasty::schema::Model,
    {
        let filters = filters.into_filters();
        self.filters = filters;
        self
    }

    /// Include every relation this table's columns declare, once each.
    pub(crate) fn include_relations(
        &self,
        mut query: toasty::stmt::Query<List<M>>,
    ) -> toasty::stmt::Query<List<M>>
    where
        M: toasty::schema::Model,
    {
        let mut seen: Vec<toasty_core::stmt::Include> = Vec::new();
        for include in self.columns.iter().flat_map(|c| c.includes().into_vec()) {
            if !seen.contains(&include) {
                query = query.include(include.clone());
                seen.push(include);
            }
        }
        query
    }

    /// Filter predicate for the current `TableState`.
    pub(crate) fn filter_expr(&self, state: &TableState) -> Option<Expr<bool>>
    where
        M: toasty::schema::Model,
    {
        let mut exprs = Vec::new();
        for f in &self.filters {
            if let Some(v) = state.filters.get(f.name())
                && let Some(e) = f.to_expr(v)
            {
                exprs.push(e);
            }
        }
        if exprs.is_empty() {
            None
        } else {
            let mut iter = exprs.into_iter();
            let first = iter.next().unwrap();
            Some(iter.fold(first, |acc, e| acc.and(e)))
        }
    }

    /// Requested filters that produce no predicate.
    pub(crate) fn unapplied_filters(&self, state: &TableState) -> Vec<(String, String)>
    where
        M: toasty::schema::Model,
    {
        let mut out = Vec::new();
        for (key, value) in &state.filters {
            match self.filters.iter().find(|f| f.name() == key) {
                None => out.push((format!("{key}:{value}"), "unknown filter".to_string())),
                Some(f) if f.to_expr(value).is_none() && !f.is_noop_value(value) => {
                    out.push((format!("{key}:{value}"), "invalid value".to_string()))
                }
                Some(_) => {}
            }
        }
        if state.filters_dropped {
            out.push((
                "dropped filters".to_string(),
                format!(
                    "more than {}, over {} bytes, or the retired filters= form",
                    state::MAX_FILTERS,
                    state::MAX_FILTER_LEN
                ),
            ));
        }
        out.sort();
        out
    }

    /// Offer grouping the page's rows by the field `lens` reads, under the field's name.
    pub fn group_by<T>(mut self, lens: Lens<M, T>) -> Self
    where
        M: toasty::schema::Model + Send + Sync + 'static,
        T: FormScalar + Send + Sync + 'static,
    {
        self.group_by = Some(GroupDef {
            binding: Binding::of(lens.path()),
            key: Arc::new(move |record| lens.read(record).to_form()),
        });
        self
    }

    /// The declared grouping iff `state.group_by` names it.
    fn effective_group_key(&self, state: &TableState) -> Option<GroupKey<M>> {
        match (&self.group_by, &state.group_by) {
            (Some(def), Some(want)) if def.binding.name() == want => Some(def.key.clone()),
            _ => None,
        }
    }

    /// Normalize `state.group_by` against the declared grouping.
    pub(crate) fn normalize_state(&self, state: &TableState) -> TableState {
        let mut out = state.clone();
        if self.group_by.as_ref().map(|def| def.binding.name()) != out.group_by.as_deref() {
            out.group_by = None;
        }
        out
    }

    /// Set the page size.
    pub fn paginate(mut self, per_page: usize) -> Self {
        match NonZeroUsize::new(per_page) {
            Some(size) => self.page_size = size,
            None => self.misdeclared.push(DeclarationErrorKind::ZeroPageSize),
        }
        self
    }

    /// What is wrong with this declaration.
    pub fn declaration_errors(&self) -> Vec<DeclarationErrorKind> {
        let mut errors = self.misdeclared.clone();
        errors.extend(
            self.group_by
                .as_ref()
                .and_then(|def| def.binding.misdeclared()),
        );
        if self.columns.is_empty() {
            errors.push(DeclarationErrorKind::NoColumns);
        }
        let mut seen = std::collections::HashSet::with_capacity(self.columns.len());
        for column in &self.columns {
            match column.misdeclared() {
                Some(error) => errors.push(error),
                None if !seen.insert(column.name()) => {
                    errors.push(DeclarationErrorKind::DuplicateColumn {
                        name: column.name().to_string(),
                    });
                }
                None => {}
            }
        }
        let mut seen = std::collections::HashSet::with_capacity(self.filters.len());
        for filter in &self.filters {
            match filter.misdeclared() {
                Some(error) => errors.push(error),
                None if !seen.insert(filter.name()) => {
                    errors.push(DeclarationErrorKind::DuplicateFilter {
                        name: filter.name().to_string(),
                    });
                }
                None => {}
            }
        }
        errors
    }

    /// Binds the table's embedded paths to `db`'s app schema.
    ///
    /// A panel binds the tables it mounts; bind one built outside a panel so its
    /// [`declaration_errors`](Self::declaration_errors) reports no unbound path. A table with no
    /// embedded path is bound from the start.
    pub fn bind(self, db: &toasty::Db) -> Self {
        self.bind_with(&FieldResolver::of_db(db));
        self
    }

    /// Binds every column, filter, and the grouping through `resolver`.
    pub(crate) fn bind_with(&self, resolver: &FieldResolver) {
        for column in &self.columns {
            column.bind(resolver);
        }
        for filter in &self.filters {
            filter.bind(resolver);
        }
        if let Some(def) = &self.group_by {
            def.binding.bind(resolver);
        }
    }

    /// The page size.
    pub fn page_size(&self) -> usize {
        self.page_size.get()
    }

    /// The key of `record`'s row: its primary key's URL id.
    pub(crate) fn key_of(&self, record: &M) -> String {
        (self.key)(record)
    }

    /// Render no search toolbar in the table.
    pub fn hide_search(mut self) -> Self {
        self.hide_search = true;
        self
    }

    /// Render no filter bar in the table.
    pub fn hide_filter_bar(mut self) -> Self {
        self.hide_filter_bar = true;
        self
    }

    /// Global search predicate across searchable columns.
    pub(crate) fn search_expr(&self, term: &str) -> Option<Expr<bool>>
    where
        M: toasty::schema::Model,
    {
        let t = term.trim();
        if t.is_empty() {
            return None;
        }
        let mut exprs = self.columns.iter().filter_map(|c| c.search_expr(t));
        let first = exprs.next()?;
        Some(exprs.fold(first, |acc, e| acc.or(e)))
    }

    /// First sortable column's order_by (tokio-rs/toasty#1142).
    pub(crate) fn order_by(&self, descending: bool) -> Option<OrderByExpr>
    where
        M: toasty::schema::Model,
    {
        self.columns.iter().find_map(|c| c.order_by(descending))
    }

    /// Order-bys over the model's primary key.
    fn pk_order_bys() -> Vec<OrderByExpr>
    where
        M: toasty::schema::Model,
    {
        let app_model = M::schema();
        let Some(root) = app_model.as_root() else {
            debug_assert!(
                false,
                "pk_order_bys: {} is not a root model; deterministic pagination needs its primary key",
                std::any::type_name::<M>()
            );
            return Vec::new();
        };
        root.primary_key
            .fields
            .iter()
            .map(|fid| M::path_field::<toasty::stmt::Value>(fid.index).asc())
            .collect()
    }

    /// Resolve the full query ordering for a request.
    pub(crate) fn order_bys_for(&self, state: &TableState) -> Vec<OrderByExpr>
    where
        M: toasty::schema::Model,
    {
        if let Some(sort) = &state.sort
            && let Some(col) = self
                .columns
                .iter()
                .find(|c| c.is_sortable() && c.name() == sort.column)
            && let Some(ord) = col.order_by(sort.descending)
        {
            return vec![ord];
        }
        let out: Vec<OrderByExpr> = self.order_by(false).into_iter().collect();
        if out.is_empty() {
            return Self::pk_order_bys();
        }
        out
    }

    /// Apply this table's declaration to `query`.
    pub(crate) fn apply_declaration(
        &self,
        mut query: toasty::stmt::Query<List<M>>,
        state: &TableState,
    ) -> toasty::stmt::Query<List<M>>
    where
        M: toasty::schema::Model,
    {
        if let Some(term) = &state.search
            && let Some(expr) = self.search_expr(term)
        {
            query = query.filter(expr);
        }
        if let Some(expr) = self.filter_expr(state) {
            query = query.filter(expr);
        }
        for ord in self.order_bys_for(state) {
            query = query.order_by(ord);
        }
        query
    }

    /// Whether the declaration enables a search toolbar.
    pub(crate) fn search_enabled(&self) -> bool
    where
        M: toasty::schema::Model,
    {
        !self.hide_search && self.columns.iter().any(|c| c.is_searchable())
    }

    /// Whether the declaration enables a filter bar.
    pub(crate) fn filter_bar_enabled(&self) -> bool {
        !self.hide_filter_bar && !self.filters.is_empty()
    }
}

#[cfg(test)]
mod tests;
