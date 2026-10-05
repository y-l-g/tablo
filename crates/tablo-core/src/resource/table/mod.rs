//! The [`Table`] builder plus query planning (`filter_expr`/`search_expr`/`order_bys_for`).
//!
//! Rendering lives in [`render`](self::render), CSV export in [`export`](self::export).

use std::{marker::PhantomData, num::NonZeroUsize, sync::Arc};

use toasty::stmt::{Expr, List, OrderByExpr};

use super::{
    column::{BoxColumn, Column, IntoColumns},
    filter::{BoxFilter, IntoFilters},
    state::{TableState, with_return},
};
use crate::{Lens, form::FormScalar, schema::ResolvedLens};

mod export;
mod render;

pub(crate) use render::TABLE_CARD_CLASS;

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
    name: String,
    key: GroupKey<M>,
}

impl<M> Clone for GroupDef<M> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
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
    pub(crate) allowed: Arc<dyn Fn(&M) -> bool + Send + Sync>,
}

impl<M> Clone for TableAction<M> {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            label: self.label.clone(),
            row: self.row,
            bulk: self.bulk,
            allowed: Arc::clone(&self.allowed),
        }
    }
}

/// The page size of a table that declares none with [`Table::paginate`].
pub const DEFAULT_PAGE_SIZE: NonZeroUsize = NonZeroUsize::new(25).unwrap();

/// Table description of a `Resource`'s list view declaring columns and how they map to queries.
pub struct Table<M> {
    columns: Vec<BoxColumn<M>>,
    /// Misdeclarations a builder recorded ([`Self::declaration_errors`]).
    misdeclared: Vec<String>,
    filters: Vec<BoxFilter<M>>,
    group_by: Option<GroupDef<M>>,
    /// A row's key: its record's primary key, as its action URLs carry it.
    key: fn(&M) -> String,
    /// Whether a row's key resolves as a URL id; a composite key does not, so its rows carry no
    /// action.
    addressable: bool,
    row_policy: Option<RowPolicy<M>>,
    page_size: NonZeroUsize,
    hide_search: bool,
    hide_filter_bar: bool,
    delete_prefix: Option<String>,
    edit_prefix: Option<String>,
    view_prefix: Option<String>,
    bulk_delete: bool,
    /// The custom actions and the list URL their routes hang off.
    custom_actions: Vec<TableAction<M>>,
    actions_prefix: Option<String>,
    live_search: bool,
    /// Whether the table draws its own card.
    framed: bool,
    /// Where a write this table's row and bulk actions start lands.
    return_to: Option<String>,
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
            row_policy: self.row_policy.clone(),
            page_size: self.page_size,
            hide_search: self.hide_search,
            hide_filter_bar: self.hide_filter_bar,
            delete_prefix: self.delete_prefix.clone(),
            edit_prefix: self.edit_prefix.clone(),
            view_prefix: self.view_prefix.clone(),
            bulk_delete: self.bulk_delete,
            custom_actions: self.custom_actions.clone(),
            actions_prefix: self.actions_prefix.clone(),
            live_search: self.live_search,
            framed: self.framed,
            return_to: self.return_to.clone(),
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
            .field("row_policy", &self.row_policy.is_some())
            .field("page_size", &self.page_size)
            .field("hide_search", &self.hide_search)
            .field("hide_filter_bar", &self.hide_filter_bar)
            .field("delete_prefix", &self.delete_prefix)
            .field("edit_prefix", &self.edit_prefix)
            .field("view_prefix", &self.view_prefix)
            .field("bulk_delete", &self.bulk_delete)
            .field(
                "custom_actions",
                &self
                    .custom_actions
                    .iter()
                    .map(|a| a.name)
                    .collect::<Vec<_>>(),
            )
            .field("live_search", &self.live_search)
            .field("framed", &self.framed)
            .field("return_to", &self.return_to)
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
            row_policy: None,
            page_size: DEFAULT_PAGE_SIZE,
            hide_search: false,
            hide_filter_bar: false,
            delete_prefix: None,
            edit_prefix: None,
            view_prefix: None,
            bulk_delete: false,
            custom_actions: Vec::new(),
            actions_prefix: None,
            live_search: false,
            framed: true,
            return_to: None,
            _marker: PhantomData,
        }
    }

    /// Append `column` after the declared ones.
    pub fn column(mut self, column: impl Column<M> + 'static) -> Self {
        self.columns.push(Arc::new(column));
        self
    }

    /// Declare the per-record action policy.
    pub(crate) fn row_actions(
        mut self,
        policy: impl Fn(&M) -> RowActions + Send + Sync + 'static,
    ) -> Self {
        self.row_policy = Some(Arc::new(policy));
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
                    super::state::MAX_FILTERS,
                    super::state::MAX_FILTER_LEN
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
        let binding = ResolvedLens::of(lens.path().clone());
        self.misdeclared.extend(binding.misdeclared);
        self.group_by = Some(GroupDef {
            name: binding.name,
            key: Arc::new(move |record| lens.read(record).to_form()),
        });
        self
    }

    /// The declared grouping iff `state.group_by` names it.
    fn effective_group_key(&self, state: &TableState) -> Option<GroupKey<M>> {
        match (&self.group_by, &state.group_by) {
            (Some(def), Some(want)) if def.name == *want => Some(def.key.clone()),
            _ => None,
        }
    }

    /// Normalize `state.group_by` against the declared grouping.
    pub(crate) fn normalize_state(&self, state: &TableState) -> TableState {
        let mut out = state.clone();
        if self.group_by.as_ref().map(|def| def.name.as_str()) != out.group_by.as_deref() {
            out.group_by = None;
        }
        out
    }

    /// Set the page size.
    pub fn paginate(mut self, per_page: usize) -> Self {
        match NonZeroUsize::new(per_page) {
            Some(size) => self.page_size = size,
            None => self
                .misdeclared
                .push("Table::paginate: a page size must be at least 1".to_string()),
        }
        self
    }

    /// What is wrong with this declaration.
    pub fn declaration_errors(&self) -> Vec<String> {
        let mut errors = self.misdeclared.clone();
        if self.columns.is_empty() {
            errors.push(
                "a Table needs at least one column: declare columns with `Table::new(columns)` or in `Resource::table`".to_string(),
            );
        }
        let mut seen = std::collections::HashSet::with_capacity(self.columns.len());
        for column in &self.columns {
            match column.misdeclared() {
                Some(error) => errors.push(error),
                None if !seen.insert(column.name()) => errors.push(format!(
                    "duplicate column name '{}': each Table column needs a distinct name",
                    column.name()
                )),
                None => {}
            }
        }
        let mut seen = std::collections::HashSet::with_capacity(self.filters.len());
        for filter in &self.filters {
            match filter.misdeclared() {
                Some(error) => errors.push(error),
                None if !seen.insert(filter.name()) => errors.push(format!(
                    "duplicate filter name '{}': each Table filter needs a distinct name",
                    filter.name()
                )),
                None => {}
            }
        }
        errors
    }

    /// The page size.
    pub fn page_size(&self) -> usize {
        self.page_size.get()
    }

    /// The key of `record`'s row: its primary key's URL id.
    pub(crate) fn key_of(&self, record: &M) -> String {
        (self.key)(record)
    }

    /// Which row actions `record` allows.
    pub(crate) fn actions_for(&self, record: &M) -> RowActions {
        self.row_policy
            .as_ref()
            .map_or(RowActions::ALL, |policy| policy(record))
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

    /// Render the table without its own card.
    pub(crate) fn unframed(mut self) -> Self {
        self.framed = false;
        self
    }

    /// Enable keystroke-live search via the `table_search` shard.
    pub fn live_search(mut self) -> Self {
        self.live_search = true;
        self
    }

    /// Enable row-level `Delete` action.
    pub(crate) fn with_delete(mut self, prefix: String) -> Self {
        if self.addressable {
            self.delete_prefix = Some(prefix);
        }
        self
    }

    /// Enable row-level `Edit` action.
    pub(crate) fn with_edit(mut self, prefix: String) -> Self {
        if self.addressable {
            self.edit_prefix = Some(prefix);
        }
        self
    }

    /// Enable the row-level `View` action.
    pub(crate) fn with_view(mut self, prefix: String) -> Self {
        if self.addressable {
            self.view_prefix = Some(prefix);
        }
        self
    }

    /// Enable bulk selection with `BulkDelete` action.
    pub(crate) fn with_bulk_delete(mut self, enabled: bool) -> Self {
        self.bulk_delete = enabled;
        self
    }

    /// Send the writes this table starts back to `url`.
    pub(crate) fn returning_to(mut self, url: String) -> Self {
        self.return_to = Some(url);
        self
    }

    fn action_url(&self, url: String) -> String {
        match &self.return_to {
            Some(target) => with_return(&url, target),
            None => url,
        }
    }

    /// Wire the resource's custom actions.
    pub(crate) fn with_custom_actions(
        mut self,
        prefix: String,
        actions: Vec<TableAction<M>>,
    ) -> Self {
        if self.addressable {
            self.actions_prefix = Some(prefix);
            self.custom_actions = actions;
        }
        self
    }

    fn row_custom_actions(&self) -> impl Iterator<Item = &TableAction<M>> {
        self.custom_actions
            .iter()
            .filter(|a| a.row && self.actions_prefix.is_some())
    }

    fn bulk_custom_actions(&self) -> impl Iterator<Item = &TableAction<M>> {
        self.custom_actions
            .iter()
            .filter(|a| a.bulk && self.actions_prefix.is_some())
    }

    fn bulk_delete_enabled(&self) -> bool {
        self.bulk_delete && self.delete_prefix.is_some()
    }

    fn bulk_enabled(&self) -> bool {
        self.bulk_delete_enabled() || self.bulk_custom_actions().next().is_some()
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

    /// Whether the search toolbar renders.
    pub(crate) fn search_enabled(&self) -> bool
    where
        M: toasty::schema::Model,
    {
        !self.hide_search && self.columns.iter().any(|c| c.is_searchable())
    }

    /// Whether this table renders the keystroke-live search host.
    pub(crate) fn is_live_search(&self) -> bool {
        self.live_search
    }

    /// Whether the filter bar renders inside the table.
    pub(crate) fn filter_bar_enabled(&self) -> bool {
        !self.hide_filter_bar && !self.filters.is_empty()
    }
}

#[cfg(test)]
mod tests;
