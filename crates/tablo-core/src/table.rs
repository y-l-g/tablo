//! A resource's list view: the [`Table`] builder and its columns and filters, the URL state a list
//! reads, query planning (`filter_expr`/`search_expr`/`order_bys_for`), the page a table loads, and
//! CSV export.
//!
//! A column renders its own cell and a filter its own control. What a request wires onto a table
//! and the list page around it belong to the panel: [`WiredTable`](crate::panel::WiredTable).

use std::{num::NonZeroUsize, sync::Arc};

use derive_where::derive_where;
use toasty::stmt::List;
use topcoat::context::Cx;

use self::filter::BoxFilter;
pub(crate) use self::{applied::ignored_reason, choice::ColumnChoice};
use crate::DeclarationErrorKind;
pub(crate) use crate::column::{BoxColumn, Column, include_relations, restrict_relations};

mod applied;
mod choice;
mod export;
pub(crate) mod filter;
mod group;
mod mask;
pub(crate) mod model;
mod page;
mod row_color;
mod search;
mod sort;
mod state;
mod tab;

use self::sort::FieldOrder;
pub use self::{
    filter::{DateFilter, FilterValue, IntoFilters, QueryFilter, SelectFilter, TernaryFilter},
    model::ModelColumns,
    page::{DEFAULT_PAGE_SIZE, DEFAULT_PER_PAGE_OPTIONS, PagePosition, Pagination, TablePage},
    state::{Cursor, Sort, TableState},
    tab::ListTab,
};
pub(crate) use self::{
    group::GroupDef,
    state::{bulk_key, bulk_token, group_header_dom_id, query_of, request_query, row_dom_id},
};
pub(crate) use crate::{
    column::{ColumnToggle, ColumnWidth, IntoColumns, contains_expr},
    display::Alignment,
};

/// Where a table puts a row's actions: its custom actions, View, Edit and Delete.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
/// use tablo_core::{ActionLayout, Table, TextColumn, lens};
///
/// Table::new(TextColumn::new(lens!(Post.title))).action_layout(ActionLayout::Menu);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ActionLayout {
    /// Side by side at the end of the row: View, Edit and Delete as icons, a custom action by its
    /// label.
    #[default]
    Inline,
    /// In a menu behind one button at the end of the row, Delete last.
    Menu,
}

/// Table description of a `Resource`'s list view declaring columns and how they map to queries.
///
/// A clone shares every column, filter and projection.
///
/// The list keeps its whole state in the URL: `?tab=` picks a [tab](ListTab), `?q=` searches,
/// `?sort=` and `?dir=` sort, `?page=`, `?per_page=`, `?after=` and `?before=` page,
/// `?f.<name>=` filters and `?group_by=` groups. Each row is keyed by its record's primary key,
/// which the action URLs and bulk checkboxes carry; a composite key has no URL form, so its rows
/// render without row actions or bulk selection.
///
/// - Search matches a row any searchable column contains the term in. It escapes `%` and `_`, trims
///   the term and caps it at 128 characters, and matches case-insensitively: PostgreSQL runs it as
///   `ILIKE`, SQLite (for ASCII) and MySQL (under its default collations) as `LIKE`.
/// - Without a `?sort=` naming a sortable column, the rows sort by the
///   [`default_sort`](Self::default_sort), else by the first sortable column, ascending. The
///   primary key breaks ties, so a sort over duplicate values pages deterministically, and a page
///   number past the last page reads as the last.
/// - A filter that cannot apply, an unknown name, a value the filter rejects or one past the
///   limits, is never dropped silently: the list shows a warning banner naming it, and the export
///   answers 400. At most 32 filters apply, each with at most 64 values and parts, each name, part
///   and value at most 256 bytes. Active filters combine with AND.
/// - `?group_by=` groups the rows by one of the table's [groupings](Self::group_by), which orders
///   them by its field ahead of the sort, so a group runs on across pages; one the table does not
///   declare is ignored.
/// - The list updates in place: a search, filter, sort or page reruns the page through the runtime,
///   keeping focus and scroll, and without JavaScript its links and its search form load the page.
///   A table that fails to load shows an error state with a retry link in place of the rows.
///
/// When the policy allows `DeleteAny`, or `RunAny` for an action placed on the bulk bar, the list
/// adds a checkbox column and a bulk bar; a row that no delete or bulk action allows gets no
/// checkbox. A bulk delete takes at most [`Limits::bulk_records`](crate::Limits::bulk_records), 400
/// by default, runs in one transaction and skips the
/// records the policy refuses `Delete`. Both deletes ask first through a confirmation dialog,
/// which needs JavaScript; a POST not confirmed through it answers 400. Each lands back on the
/// list as it was left.
///
/// # Export
///
/// `GET {list}/-/export` returns the list as a CSV file named `{slug}.csv`, with the current
/// search, filters and sort applied and the relations the columns include loaded: the rows the
/// list shows, each of them, whatever the policy answers `View` of it. It delivers at most
/// [`Limits::export_rows`](crate::Limits::export_rows), 10,000 by default, and answers 413 rather
/// than a truncated file when more match. It writes each column's formatted value, a date column's
/// stored value, and escapes a cell a spreadsheet would read as a formula; `?bom=1` prefixes the
/// file with a byte-order mark for Excel.
#[derive_where(Clone)]
pub struct Table<M> {
    columns: Vec<BoxColumn<M>>,
    /// Misdeclarations a builder recorded ([`Self::declaration_errors`]).
    misdeclared: Vec<DeclarationErrorKind>,
    filters: Vec<BoxFilter<M>>,
    groupings: Vec<GroupDef<M>>,
    row_color: row_color::RowColor<M>,
    tabs: Vec<ListTab<M>>,
    /// A row's key: its record's primary key, as its action URLs carry it.
    key: fn(&M) -> String,
    /// Whether a row's key resolves as a URL id; a composite key does not, so its rows carry no
    /// action.
    addressable: bool,
    page_size: NonZeroUsize,
    pagination: Pagination,
    action_layout: ActionLayout,
    /// The page sizes `?per_page=` may ask for, sorted; empty offers no choice.
    per_page_options: Vec<usize>,
    /// The sort a request naming no sortable column reads.
    default_sort: Option<Sort>,
    /// The field a [`default_sort`](Self::default_sort) orders by, which no sortable column
    /// need show.
    default_field: Option<FieldOrder>,
    hide_search: bool,
    hide_filter_menu: bool,
    /// Whether the filter menu applies its controls only on its Apply button.
    deferred_filters: bool,
}

impl<M> std::fmt::Debug for Table<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Table")
            .field(
                "columns",
                &self.columns.iter().map(|c| c.name()).collect::<Vec<_>>(),
            )
            .field("filters", &self.filters.len())
            .field(
                "groupings",
                &self
                    .groupings
                    .iter()
                    .map(GroupDef::name)
                    .collect::<Vec<_>>(),
            )
            .field(
                "tabs",
                &self.tabs.iter().map(ListTab::key).collect::<Vec<_>>(),
            )
            .field("page_size", &self.page_size)
            .field("pagination", &self.pagination)
            .field("default_sort", &self.default_sort)
            .field("hide_search", &self.hide_search)
            .field("hide_filter_menu", &self.hide_filter_menu)
            .field("deferred_filters", &self.deferred_filters)
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
            groupings: Vec::new(),
            row_color: row_color::RowColor::default(),
            tabs: Vec::new(),
            key: crate::toasty_compat::pk::pk_text::<M>,
            addressable: !crate::toasty_compat::pk::pk_is_composite::<M>(),
            page_size: DEFAULT_PAGE_SIZE,
            pagination: Pagination::default(),
            action_layout: ActionLayout::default(),
            per_page_options: DEFAULT_PER_PAGE_OPTIONS.to_vec(),
            default_sort: None,
            default_field: None,
            hide_search: false,
            hide_filter_menu: false,
            deferred_filters: false,
        }
    }

    /// Append `column` after the declared ones.
    pub fn column(mut self, column: impl Column<M> + 'static) -> Self {
        self.columns.push(Arc::new(column));
        self
    }

    /// Append `filters` after the declared ones, such as those a field's `#[columns(filter)]`
    /// declares.
    pub fn filters(mut self, filters: impl IntoFilters<M>) -> Self
    where
        M: toasty::schema::Model,
    {
        self.filters.extend(filters.into_filters());
        self
    }

    /// Whether a reader may hide or show any of the columns.
    pub(crate) fn has_toggleable_columns(&self) -> bool {
        self.columns.iter().any(|c| c.toggle().is_toggleable())
    }

    /// The declared columns, in order.
    pub(crate) fn declared_columns(&self) -> &[BoxColumn<M>] {
        &self.columns
    }

    /// This table with only the columns `names` names, in that order, and no default sort on a
    /// column it drops. A name it declares no column for picks none.
    pub(crate) fn only(&self, names: &[String]) -> Self {
        let mut table = self.clone();
        table.columns = names
            .iter()
            .filter_map(|name| self.columns.iter().find(|column| column.name() == name))
            .cloned()
            .collect();
        if table
            .default_sort
            .as_ref()
            .is_some_and(|sort| !table.orders_by(sort))
        {
            table.default_sort = None;
        }
        table
    }

    /// Appends `filter` after the declared ones.
    pub(crate) fn push_filter(&mut self, filter: BoxFilter<M>) {
        self.filters.push(filter);
    }

    /// The declared filters, in order.
    pub(crate) fn declared_filters(&self) -> &[BoxFilter<M>] {
        &self.filters
    }

    /// Include every relation this table's columns declare, once each.
    pub(crate) fn include_relations(
        &self,
        query: toasty::stmt::Query<List<M>>,
    ) -> toasty::stmt::Query<List<M>>
    where
        M: toasty::schema::Model,
    {
        include_relations(query, &self.columns)
    }

    /// Whether [`Self::include_relations`] adds an include.
    pub(crate) fn includes_relations(&self) -> bool {
        self.columns
            .iter()
            .any(|column| !column.includes().into_vec().is_empty())
    }

    /// Check which related records `rows`, loaded through [`Self::include_relations`], hold that
    /// the columns' sources show the user.
    pub(crate) async fn restrict_relations(
        &self,
        cx: &topcoat::context::Cx,
        rows: &[M],
        ex: &mut dyn toasty::Executor,
    ) -> topcoat::Result<()> {
        restrict_relations(cx, &self.columns, rows, ex).await
    }

    /// Page through the rows as `pagination` says; [`Pagination::Pages`] by default.
    pub fn pagination(mut self, pagination: Pagination) -> Self {
        self.pagination = pagination;
        self
    }

    /// Put a row's actions where `layout` says; [`ActionLayout::Inline`] by default.
    pub fn action_layout(mut self, layout: ActionLayout) -> Self {
        self.action_layout = layout;
        self
    }

    /// The page sizes a reader may pick, beside the table's own; none offers no choice.
    /// [`DEFAULT_PER_PAGE_OPTIONS`] by default. A cursor table offers none.
    pub fn per_page_options(mut self, options: impl IntoIterator<Item = usize>) -> Self {
        self.per_page_options = options.into_iter().filter(|size| *size > 0).collect();
        self
    }

    /// Show `size` rows a page; [`DEFAULT_PAGE_SIZE`] by default.
    pub fn page_size(mut self, size: usize) -> Self {
        match NonZeroUsize::new(size) {
            Some(size) => self.page_size = size,
            None => self.misdeclared.push(DeclarationErrorKind::ZeroPageSize),
        }
        self
    }

    /// The columns and filters reading a source the context's panel cannot load from.
    pub(crate) fn unavailable_sources(
        &self,
        cx: &topcoat::context::Cx,
    ) -> Vec<DeclarationErrorKind> {
        let mut errors = crate::column::unavailable_sources(cx, self.columns.iter().map(|c| &**c));
        errors.extend(self.filters.iter().filter_map(|filter| {
            filter.internals().unavailable_source(cx).map(|source| {
                DeclarationErrorKind::UnregisteredOptionSource {
                    field: filter.name().to_string(),
                    source,
                }
            })
        }));
        errors
    }

    /// What is wrong with this declaration.
    pub fn declaration_errors(&self) -> Vec<DeclarationErrorKind> {
        let mut errors = self.misdeclared.clone();
        errors.extend(self.grouping_errors());
        errors.extend(self.tab_errors());
        if self.columns.is_empty() {
            errors.push(DeclarationErrorKind::NoColumns);
        }
        if let Some(sort) = &self.default_sort
            && !self.orders_by(sort)
        {
            errors.push(DeclarationErrorKind::UnsortableDefaultSort {
                column: sort.column.clone(),
            });
        }
        let mut seen = std::collections::HashSet::with_capacity(self.columns.len());
        for column in &self.columns {
            match column.internals().misdeclared() {
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
            match filter.internals().misdeclared() {
                Some(error) => errors.push(error),
                None if filter.name().contains('.') => {
                    errors.push(DeclarationErrorKind::DottedFilterName {
                        name: filter.name().to_string(),
                    });
                }
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

    /// The rows a page shows, as [`Self::page_size`] set them.
    pub fn per_page(&self) -> usize {
        self.page_size.get()
    }

    /// How the table pages.
    pub(crate) fn pagination_mode(&self) -> Pagination {
        self.pagination
    }

    /// Where the table puts a row's actions.
    pub(crate) fn row_action_layout(&self) -> ActionLayout {
        self.action_layout
    }

    /// The page sizes the reader may pick, sorted, the table's own among them; empty when it
    /// offers no choice.
    pub(crate) fn page_sizes(&self) -> Vec<usize> {
        if self.per_page_options.is_empty() || self.pagination == Pagination::Cursor {
            return Vec::new();
        }
        let mut sizes = self.per_page_options.clone();
        sizes.push(self.per_page());
        sizes.sort_unstable();
        sizes.dedup();
        sizes
    }

    /// The page size `state` reads at: its `?per_page=` when the table offers it, else the
    /// table's own.
    pub(crate) fn page_size_for(&self, state: &TableState) -> usize {
        state
            .per_page
            .and_then(|asked| usize::try_from(asked).ok())
            .filter(|asked| self.page_sizes().contains(asked))
            .unwrap_or_else(|| self.per_page())
    }

    /// The key of `record`'s row: its primary key's URL id.
    pub(crate) fn key_of(&self, record: &M) -> String {
        (self.key)(record)
    }

    /// Whether a row's key resolves as a URL id, so its record's actions have a route.
    pub(crate) fn is_addressable(&self) -> bool {
        self.addressable
    }

    /// Render no search toolbar in the table.
    pub fn hide_search(mut self) -> Self {
        self.hide_search = true;
        self
    }

    /// Render no filter menu in the table's toolbar.
    pub fn hide_filter_menu(mut self) -> Self {
        self.hide_filter_menu = true;
        self
    }

    /// Apply the filter menu's controls only when its Apply button is pressed, rather than as each
    /// one changes, so a reader sets several filters before the list reruns once. A search, a
    /// sort or a tab applies the controls as they stand too, since the toolbar's form carries
    /// them.
    pub fn deferred_filters(mut self) -> Self {
        self.deferred_filters = true;
        self
    }

    /// Whether the filter menu applies its controls only on its Apply button.
    pub(crate) fn filters_deferred(&self) -> bool {
        self.deferred_filters
    }

    /// Apply this table's declaration to `query`: [`Self::narrow`], then the ordering.
    pub(crate) fn apply_declaration(
        &self,
        cx: &Cx,
        db: &toasty::Db,
        query: toasty::stmt::Query<List<M>>,
        state: &TableState,
        refused: &[String],
    ) -> toasty::stmt::Query<List<M>>
    where
        M: toasty::schema::Model,
    {
        self.order(self.narrow(cx, db, query, state, refused), state)
    }

    /// Narrow `query` to the rows `state`'s tab, search and filters select, but the `refused`
    /// filters, unordered: what a count reads.
    pub(crate) fn narrow(
        &self,
        cx: &Cx,
        db: &toasty::Db,
        query: toasty::stmt::Query<List<M>>,
        state: &TableState,
        refused: &[String],
    ) -> toasty::stmt::Query<List<M>>
    where
        M: toasty::schema::Model,
    {
        self.narrow_tab(self.narrow_untabbed(cx, db, query, state, refused), state)
    }

    /// [`Self::narrow`] but the tab: what a tab's badge counts within.
    fn narrow_untabbed(
        &self,
        cx: &Cx,
        db: &toasty::Db,
        mut query: toasty::stmt::Query<List<M>>,
        state: &TableState,
        refused: &[String],
    ) -> toasty::stmt::Query<List<M>>
    where
        M: toasty::schema::Model,
    {
        if let Some(term) = &state.search
            && let Some(expr) = self.search_expr(term)
        {
            query = query.filter(crate::toasty_compat::case_insensitive_search(db, expr));
        }
        if let Some(expr) = self.filter_expr(cx, state, refused) {
            query = query.filter(expr);
        }
        query
    }

    /// Whether the declaration enables a filter menu.
    pub(crate) fn filter_menu_enabled(&self) -> bool {
        !self.hide_filter_menu && !self.filters.is_empty()
    }
}

#[cfg(test)]
mod tests;
