//! The [`Table`] builder plus query planning (`filter_expr`/`search_expr`/`order_bys_for`).
//!
//! Rendering lives in [`render`](self::render), CSV export in [`export`](self::export).
//! One routine applies the declaration for both loaders, and [`Table::paginate`]
//! refuses a page size of zero where it is declared.

use std::{marker::PhantomData, num::NonZeroUsize, sync::Arc};

use toasty::stmt::{Expr, List, OrderByExpr};

use super::{
    column::{BoxColumn, Column, IntoColumns},
    filter::{BoxFilter, IntoFilters},
    state::{TableState, with_return},
};

mod export;
mod render;

pub(crate) use render::TABLE_CARD_CLASS;

/// Row-key projection: reads the row identity off one model instance
/// (typically `|u| u.id.to_string()`). Toasty models are plain structs with
/// no instance→field reflection, so the key cannot be extracted generically
/// (upstream gap #119).
pub type RowKey<M> = Arc<dyn Fn(&M) -> String + Send + Sync>;

/// Group-label projection: reads a row's group off one model instance
/// (typically `|u| u.status.clone()`).
pub type GroupKey<M> = Arc<dyn Fn(&M) -> String + Send + Sync>;

/// Per-record action policy: reads which row actions one model instance allows.
/// See [`RowActions`].
pub(crate) type RowPolicy<M> = Arc<dyn Fn(&M) -> RowActions + Send + Sync>;

/// Which row actions one record may use.
///
/// The action prefixes say which affordances a resource declares; this says
/// which of them the caller may use
/// on one loaded row. The panel wires the projection into the table and the
/// renderer consults it per row — a denied action emits no link, and a row
/// denied `delete` renders no bulk checkbox, so the row can never enter the
/// selection transport.
///
/// The panel derives one from the resource's
/// [`can_view`](crate::resource::Resource::can_view) /
/// [`can_update`](crate::resource::Resource::can_update) /
/// [`can_delete`](crate::resource::Resource::can_delete), pairing each action
/// with the same predicates its route checks, so a rendered affordance and the
/// route that answers it cannot disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RowActions {
    /// Whether the row renders its `View` link — the detail route's
    /// `can_view`.
    pub view: bool,
    /// Whether the row renders its `Edit` link — the edit route's `can_view`
    /// **and** `can_update`.
    pub edit: bool,
    /// Whether the row renders its `Delete` link and an enabled bulk checkbox —
    /// the delete route's `can_view` **and** `can_delete`.
    pub delete: bool,
}

impl RowActions {
    /// Every action allowed: what a table without a panel-wired row policy
    /// renders.
    pub const ALL: Self = Self {
        view: true,
        edit: true,
        delete: true,
    };
}

/// A named grouping a `Table` can render: `name` is the `?group_by=` value
/// the table accepts, `key` projects a row to its group label.
pub(crate) struct GroupDef<M> {
    name: String,
    key: GroupKey<M>,
}

// Hand-written: a derive would require `M: Clone`, and every field is an
// `Arc` or owned data whatever `M` is.
impl<M> Clone for GroupDef<M> {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            key: Arc::clone(&self.key),
        }
    }
}

/// The action chrome a resource declares: which row actions
/// `wire_table_actions` attaches.
///
/// [`Resource::table`](crate::resource::Resource::table) returns a table
/// carrying no delete/edit/view prefix: the panel attaches them from the
/// resource's [`can_delete_any`](crate::resource::Resource::can_delete_any),
/// its record form, and whether it declares a
/// [`view`](crate::resource::Resource::view).
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

/// One custom [`Action`](crate::resource::Action) as a table renders it:
/// the button text, where it renders, and the per-record gate the panel
/// wired from [`Resource::can_view`](crate::resource::Resource::can_view) and
/// the action's `can_run`.
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

/// The page size of a table that declares none with [`Table::paginate`], as
/// Filament's tables default to paginating.
pub const DEFAULT_PAGE_SIZE: NonZeroUsize = NonZeroUsize::new(25).unwrap();

/// Table description of a `Resource`'s list view. Declares columns and how they
/// map to queries.
///
/// Row identity is mandatory and typed: [`Table::new`] takes the key
/// projection driving both key halves, and each cell renders through its
/// [`Column`].
pub struct Table<M> {
    columns: Vec<BoxColumn<M>>,
    /// Misdeclarations a builder recorded ([`Self::declaration_errors`]).
    misdeclared: Vec<String>,
    filters: Vec<BoxFilter<M>>,
    group_by: Option<GroupDef<M>>,
    row_key: RowKey<M>,
    record_key: RowKey<M>,
    row_policy: Option<RowPolicy<M>>,
    page_size: NonZeroUsize,
    hide_search: bool,
    hide_filter_bar: bool,
    delete_prefix: Option<String>,
    edit_prefix: Option<String>,
    view_prefix: Option<String>,
    bulk_delete: bool,
    /// The custom actions, and the list URL their routes hang off: set
    /// together by [`Self::with_custom_actions`].
    custom_actions: Vec<TableAction<M>>,
    actions_prefix: Option<String>,
    live_search: bool,
    /// Whether the table draws its own card: `false` where the page draws the
    /// card around the table and the controls it hoists (the live list).
    framed: bool,
    /// Where a write this table's row and bulk actions start lands:
    /// `None` for the resource's own list, the default.
    return_to: Option<String>,
    _marker: PhantomData<M>,
}

/// A copy sharing every column, filter and projection: each is an `Arc`. The
/// panel decorates a copy of the declared table per request.
impl<M> Clone for Table<M> {
    fn clone(&self) -> Self {
        Self {
            columns: self.columns.clone(),
            misdeclared: self.misdeclared.clone(),
            filters: self.filters.clone(),
            group_by: self.group_by.clone(),
            row_key: Arc::clone(&self.row_key),
            record_key: Arc::clone(&self.record_key),
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
    /// Declare a table with its row key and columns.
    ///
    /// One primary-key projection (typically `|u| u.id.to_string()`) drives
    /// both halves: keyed diffs and DOM ids, and the action URLs and bulk
    /// values handlers resolve as the model's typed PK. The projection must be
    /// injective within a page: duplicate keys corrupt keyed diffs and bulk
    /// selection, and are debug-asserted at render time. A table whose display
    /// projects a non-PK value uses [`Self::new_split`].
    ///
    /// Two columns sharing a [`Column::name`] are a misdeclaration
    /// ([`Self::declaration_errors`]): sort resolution is
    /// first-sortable-`name()`-match, so duplicate sortable names would
    /// silently misresolve `?sort=`. The rule covers computed names too
    /// (`TextColumn::computed("Status", ..)` derives `name = "status"`). So is
    /// an empty column set: a table with no columns renders a headers-only
    /// list, which no resource declares.
    pub fn new(
        key: impl Fn(&M) -> String + Send + Sync + 'static,
        cols: impl IntoColumns<M>,
    ) -> Self
    where
        M: toasty::schema::Model,
    {
        let key = Arc::new(key);
        Self::from_keys(key.clone(), key, cols)
    }

    /// Declare a table whose display projects a non-PK value.
    ///
    /// `display` drives keyed diffs and DOM ids; `record` drives action URLs
    /// and bulk checkbox values handlers resolve as the model's typed PK
    /// (`pk_eq_expr` / `pk_in_expr` — an unparseable value 404s), so emitting
    /// a display key there 404s every delete and bulk submit.
    ///
    /// Duplicate column names and an empty column set are misdeclarations,
    /// as for [`Self::new`].
    pub fn new_split(
        display: impl Fn(&M) -> String + Send + Sync + 'static,
        record: impl Fn(&M) -> String + Send + Sync + 'static,
        cols: impl IntoColumns<M>,
    ) -> Self
    where
        M: toasty::schema::Model,
    {
        Self::from_keys(Arc::new(display), Arc::new(record), cols)
    }

    /// The record key of `record`: the model's primary key as the action URLs
    /// carry it.
    pub(crate) fn record_key_of(&self, record: &M) -> String {
        (self.record_key)(record)
    }

    /// The one constructor body: the table around the two declared
    /// projections.
    fn from_keys(row_key: RowKey<M>, record_key: RowKey<M>, cols: impl IntoColumns<M>) -> Self
    where
        M: toasty::schema::Model,
    {
        Self {
            columns: cols.into_columns(),
            misdeclared: Vec::new(),
            filters: Vec::new(),
            group_by: None,
            row_key,
            record_key,
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

    /// Append `column` after the declared ones: the way to a column past the
    /// eight a tuple holds, or a single app [`Column`] without a one-element
    /// tuple.
    ///
    /// A column whose [`Column::name`] another column already has is a
    /// misdeclaration, as for [`Self::new`].
    pub fn column(mut self, column: impl Column<M> + 'static) -> Self {
        self.columns.push(Arc::new(column));
        self
    }

    /// Declare the per-record action policy: which of the wired row
    /// actions each record may use.
    ///
    /// The `with_*` methods say which affordances the table declares; this says
    /// which of them a row may use. The renderer consults it per row — a denied
    /// action emits no link, and a row denied `delete` renders no bulk
    /// checkbox, so the selection transport never carries a key the handler
    /// refuses.
    ///
    /// Defaults to [`RowActions::ALL`], so a table that declares no policy
    /// renders exactly the chrome its `with_*` calls attached. The policy is
    /// consulted only where chrome is wired: a table with no action prefix
    /// never calls it, and a resource that declares no chrome keeps its list
    /// page free of per-record predicate calls.
    ///
    /// The panel wires this from
    /// [`can_view`](crate::resource::Resource::can_view) /
    /// [`can_update`](crate::resource::Resource::can_update) /
    /// [`can_delete`](crate::resource::Resource::can_delete), each action
    /// mirroring the predicates its route checks.
    pub(crate) fn row_actions(
        mut self,
        policy: impl Fn(&M) -> RowActions + Send + Sync + 'static,
    ) -> Self {
        self.row_policy = Some(Arc::new(policy));
        self
    }

    /// Declare filters. Accepts a single filter or tuple of filters.
    ///
    /// Two filters sharing a [`Filter::name`](super::Filter::name) are a
    /// misdeclaration, as for [`Self::new`]: a filter travels as one
    /// `?f.<name>=` parameter, and the parser keeps the first value for a
    /// repeated name, so two filters sharing a name would silently drop one of
    /// them.
    pub fn filters(mut self, filters: impl IntoFilters<M>) -> Self
    where
        M: toasty::schema::Model,
    {
        let filters = filters.into_filters();
        self.filters = filters;
        self
    }

    /// `query` with every relation this table's columns declared
    /// ([`Column::includes`]) included, once
    /// each. The list and the export load through this; every column renders,
    /// so the set is the union over all of them.
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

    /// Filter predicate for the current `TableState` — `AND` of active filter exprs.
    pub fn filter_expr(&self, state: &TableState) -> Option<Expr<bool>>
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
            // `Expr::and` chain: first and all.
            let mut iter = exprs.into_iter();
            let first = iter.next().unwrap();
            Some(iter.fold(first, |acc, e| acc.and(e)))
        }
    }

    /// Requested filters that produced no predicate: `(key:value, reason)`
    /// where reason is `"unknown filter"` (no declared filter owns the key)
    /// or `"invalid value"` (the declared filter rejected the value).
    ///
    /// Documented no-op values are exempt: `TernaryFilter`'s `all`
    /// selects no predicate by contract, so it is never flagged.
    ///
    /// Filter parameters the parse dropped ([`TableState::filters_dropped`]:
    /// too many, too long, or the retired `?filters=` spelling) are reported
    /// as one entry with their own reason.
    ///
    /// The list view renders these as a `role=alert` banner and keeps a 200;
    /// the export refuses the request with 400 instead of silently
    /// over-sharing an effectively-unfiltered CSV.
    pub fn unapplied_filters(&self, state: &TableState) -> Vec<(String, String)>
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

    /// Group rows in-memory by a named key (count summarizer). No GROUP BY SQL.
    ///
    /// `name` declares the `?group_by=` value this table accepts
    /// (e.g. `"status"`); any other value renders no group headers and is
    /// dropped from pager/sort/filter links instead of silently
    /// grouping by the single declared key. Counts are page-local.
    ///
    /// The key closure reads the loaded row, so a relation it reads must be
    /// loaded: include it on a column ([`TextColumn::include`](super::TextColumn::include),
    /// [`Column::includes`]) or in [`Resource::query`](crate::resource::Resource::query).
    ///
    /// In live tables `group_by` travels in the query signal and persists
    /// across in-place reruns; changing it is still a navigation
    /// (`?group_by=` links) until a live control ships.
    pub fn group_by(
        mut self,
        name: impl Into<String>,
        key: impl Fn(&M) -> String + Send + Sync + 'static,
    ) -> Self {
        self.group_by = Some(GroupDef {
            name: name.into(),
            key: Arc::new(key),
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

    /// Normalize `state.group_by` against the declared grouping (GH #92, GH
    /// #153): an unknown `?group_by=` value renders no group headers and is
    /// dropped from every link instead of round-tripping.
    ///
    /// The panel's list page and the `table_search` shard normalize where they
    /// parse the state and render from the result; the public
    /// [`render_with_state`](Self::render_with_state) normalizes the state it
    /// is handed, so a page calling it directly keeps the GH #153 guarantee.
    /// Normalizing twice is a no-op.
    pub(crate) fn normalize_state(&self, state: &TableState) -> TableState {
        let mut out = state.clone();
        if self.group_by.as_ref().map(|def| def.name.as_str()) != out.group_by.as_deref() {
            out.group_by = None;
        }
        out
    }

    /// Set the page size. Every table paginates with cursor pagination, at
    /// [`DEFAULT_PAGE_SIZE`] rows unless it declares otherwise; the render
    /// shows Previous/Next links built from the executed page's cursors, never
    /// page numbers.
    ///
    /// Zero is a misdeclaration ([`Self::declaration_errors`]): a page of no
    /// rows is a programmer error. The table keeps its page size.
    pub fn paginate(mut self, per_page: usize) -> Self {
        match NonZeroUsize::new(per_page) {
            Some(size) => self.page_size = size,
            None => self
                .misdeclared
                .push("Table::paginate: a page size must be at least 1".to_string()),
        }
        self
    }

    /// What is wrong with this declaration: an empty column set, two columns
    /// or two filters sharing a name, a zero page size, and what each column
    /// and filter reports of itself (a lens that binds no single field, a
    /// search or sort asked of a computed column).
    ///
    /// [`Panel::build`](crate::Panel::build) refuses a resource whose table
    /// reports any, and rendering one fails with them.
    pub fn declaration_errors(&self) -> Vec<String> {
        let mut errors = self.misdeclared.clone();
        if self.columns.is_empty() {
            errors.push(
                "a Table needs at least one column: declare columns with Table::new(key, columns)"
                    .to_string(),
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

    /// The page size: [`DEFAULT_PAGE_SIZE`] unless [`Self::paginate`] set one.
    pub fn page_size(&self) -> usize {
        self.page_size.get()
    }

    /// Which row actions `record` allows: the panel-wired policy, or
    /// [`RowActions::ALL`] when the table declares none.
    ///
    /// The renderer reads this per row to decide the View/Edit/Delete links and
    /// whether the bulk checkbox is enabled.
    pub(crate) fn actions_for(&self, record: &M) -> RowActions {
        self.row_policy
            .as_ref()
            .map_or(RowActions::ALL, |policy| policy(record))
    }

    /// Render no search toolbar in the table.
    ///
    /// The toolbar shows whenever at least one column is `searchable()`, so the
    /// toolbar and the query stay in step. The live list hides it here and
    /// renders its own above the swapped region.
    pub fn hide_search(mut self) -> Self {
        self.hide_search = true;
        self
    }

    /// Render no filter bar in the table.
    ///
    /// The bar shows whenever the table declares filters. The live list hoists
    /// the bar out of the swapped table and hides it here, as it does the
    /// search toolbar: a `<select>` that is rebuilt by its own rerun loses
    /// focus and collapses its native popup.
    pub fn hide_filter_bar(mut self) -> Self {
        self.hide_filter_bar = true;
        self
    }

    /// Render the table without its own card, for a page that draws one
    /// around it: the live list puts the hoisted search and filter bars in the
    /// same card as the table they drive.
    pub(crate) fn unframed(mut self) -> Self {
        self.framed = false;
        self
    }

    /// Keystroke-live search via the `table_search` shard.
    ///
    /// The toolbar renders a signal-backed input that
    /// re-renders the table after a short keystroke-quiet delay (GH #172,
    /// `LIVE_SEARCH_DEBOUNCE_MS`), morphing in place so focus
    /// and typing survive, instead of a GET submit. The `?q=` GET form stays
    /// inside `<noscript>` as the no-JS fallback. Opt-in per resource; the
    /// shard authorizes itself (`can_view_any` + the tenant-scoped query,
    /// GH #223) and every arg is validated like the GET path.
    /// Per-row `can_view` is not applied here, matching the list page:
    /// page-local row filtering would mislabel pagination, so row scoping
    /// belongs in `Resource::query`, inside that scope.
    /// Note: Topcoat coalesces same-tick keystrokes and aborts in-flight
    /// reruns (latest wins); the time-based debounce above composes with
    /// that (delayed writes rerun normally).
    pub fn live_search(mut self) -> Self {
        self.live_search = true;
        self
    }

    /// Enable row-level `Delete` action. When set, each row renders a
    /// `Delete` button that POSTs to `{prefix}/{id}/delete` with
    /// `requires_confirmation` semantics. `{id}` is the record key
    /// (handlers resolve it as the typed PK).
    ///
    /// The action is gated per record by the panel-wired row policy: a row
    /// the policy denies renders no `Delete` link and no bulk checkbox,
    /// matching the handler's `can_view` + `can_delete` check.
    pub(crate) fn with_delete(mut self, prefix: String) -> Self {
        self.delete_prefix = Some(prefix);
        self
    }

    /// Enable row-level `Edit` action. When set, each row renders
    /// an `Edit` link to `{prefix}/{id}/edit` (Filament's `recordActions`
    /// `EditAction`, same last-column slot as `Delete`). `{id}` is the
    /// record key.
    ///
    /// The action is gated per record by the panel-wired row policy: a row
    /// the policy denies renders no `Edit` link, matching the edit route's
    /// `can_view` + `can_update` check. The list still renders every row —
    /// `can_view` stays out of the query, so pagination is not mislabelled.
    pub(crate) fn with_edit(mut self, prefix: String) -> Self {
        self.edit_prefix = Some(prefix);
        self
    }

    /// Enable the row-level `View` action. When set, each row renders
    /// a `View` link to `{prefix}/{id}` — the detail page — in the same
    /// last-column slot as `Edit` and `Delete`. `{id}` is the record key,
    /// like the edit URL.
    ///
    /// The caller sets this only for a resource that declares a detail page
    /// ([`Resource::viewed`](crate::resource::Resource::viewed)), so a resource
    /// with no view renders no link instead of one that 404s. The action is
    /// gated per record by the panel-wired row policy: a row the policy
    /// denies renders no `View` link, matching the detail route's `can_view`.
    pub(crate) fn with_view(mut self, prefix: String) -> Self {
        self.view_prefix = Some(prefix);
        self
    }

    /// Enable bulk selection with `BulkDelete` action. Checkbox values are
    /// the record keys (handlers resolve them as typed PKs).
    ///
    /// A row the panel-wired row policy denies `delete` renders no
    /// checkbox, so select-all never submits a batch the handler's
    /// all-or-nothing check refuses.
    pub(crate) fn with_bulk_delete(mut self, enabled: bool) -> Self {
        self.bulk_delete = enabled;
        self
    }

    /// Send the writes this table's Edit, Delete and bulk actions start back
    /// to `url`, a page under the panel prefix, instead of the resource's
    /// list: a relation table on a record page returns to that page.
    pub(crate) fn returning_to(mut self, url: String) -> Self {
        self.return_to = Some(url);
        self
    }

    /// `url` carrying this table's return target, when it has one.
    fn action_url(&self, url: String) -> String {
        match &self.return_to {
            Some(target) => with_return(&url, target),
            None => url,
        }
    }

    /// Wire the resource's custom actions, whose routes hang off `prefix`,
    /// the resource's list URL.
    pub(crate) fn with_custom_actions(
        mut self,
        prefix: String,
        actions: Vec<TableAction<M>>,
    ) -> Self {
        self.actions_prefix = Some(prefix);
        self.custom_actions = actions;
        self
    }

    /// The custom actions a row renders a button for.
    fn row_custom_actions(&self) -> impl Iterator<Item = &TableAction<M>> {
        self.custom_actions
            .iter()
            .filter(|a| a.row && self.actions_prefix.is_some())
    }

    /// The custom actions the bulk bar renders a button for.
    fn bulk_custom_actions(&self) -> impl Iterator<Item = &TableAction<M>> {
        self.custom_actions
            .iter()
            .filter(|a| a.bulk && self.actions_prefix.is_some())
    }

    /// Whether bulk delete renders: bulk selection plus a delete prefix to
    /// post to.
    fn bulk_delete_enabled(&self) -> bool {
        self.bulk_delete && self.delete_prefix.is_some()
    }

    /// Whether the bulk checkbox column and bar render: bulk delete, or a
    /// bulk custom action.
    fn bulk_enabled(&self) -> bool {
        self.bulk_delete_enabled() || self.bulk_custom_actions().next().is_some()
    }

    /// Global search predicate — OR across searchable columns.
    ///
    /// Substring match (`?q=` anywhere in the value), escaped so a term
    /// containing `%` or `_` stays literal; see
    /// [`TextColumn`](crate::resource::TextColumn)'s search for the driver
    /// case-sensitivity caveat.
    pub fn search_expr(&self, term: &str) -> Option<Expr<bool>>
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

    /// First sortable column's order_by. Cursor determinism needs no
    /// app-level tie-breaker: toasty's engine appends the physical PK columns
    /// to ambiguous cursor orderings internally (`normalize_cursor_order`,
    /// tokio-rs/toasty#1142), so page contents are deterministic on SQL
    /// backends without Tablo's help.
    pub fn order_by(&self, descending: bool) -> Option<OrderByExpr>
    where
        M: toasty::schema::Model,
    {
        self.columns.iter().find_map(|c| c.order_by(descending))
    }

    /// Order-bys over the model's primary key (asc, in declared order) —
    /// built through the public facade (`Model::path_field` + `Path::asc`),
    /// no `toasty_core` needed.
    ///
    /// Used only when a table declares no sortable column at all: toasty's
    /// planner requires an `ORDER BY` for cursor pagination and its
    /// normalization only extends a non-empty ordering, so the PK order must
    /// be declared app-side in that one case.
    ///
    /// # Panics
    ///
    /// Never panics: a non-root model has no primary key, so this returns
    /// empty (and debug-asserts) instead of panicking per request —
    /// the engine then reports its descriptive "requires an ORDER BY" error
    /// at load.
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
    ///
    /// Single source of truth for loaders, render and the export:
    /// 1. `?sort=<column>&dir=asc|desc` when `<column>` names a declared sortable column — that
    ///    column's direction (toasty appends the PK tie-breakers internally, see
    ///    [`Self::order_by`]);
    /// 2. otherwise the declared default (first sortable column asc);
    /// 3. otherwise the PK alone — cursor pagination requires a deterministic order even with no
    ///    sortable column, and toasty only *extends* an existing non-empty ordering.
    ///
    /// Loaders that also need the search term parse the state once with
    /// [`TableState::from_cx`] and apply the declaration through
    /// `Self::apply_declaration` (see `crate::panel::Panel`'s generic
    /// resource list handler).
    pub fn order_bys_for(&self, state: &TableState) -> Vec<OrderByExpr>
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

    /// Apply this table's declaration to `query` — the one routine that turns
    /// the search term, the filters and the ordering into a query.
    ///
    /// `query` is the caller's seed: the list and the export both pass the
    /// tenant-scoped [`Resource::query`](crate::resource::Resource::query) (the
    /// row-scoping seam, ADR-0002), and a page-owned table passes its own. One
    /// routine for both, so a new search or filter dimension cannot reach the
    /// list and miss the CSV — the drift class GH #172 fixed.
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

    /// Whether the search toolbar renders: at least one `searchable()` column,
    /// unless [`Self::hide_search`] hid it.
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

    /// Whether the filter bar renders inside the table: the table declares at
    /// least one filter, unless [`Self::hide_filter_bar`] hid it.
    ///
    /// Live tables turn it off: the list page hoists the bar out of
    /// the swapped region, the same way it owns the search toolbar, so a filter
    /// change cannot rebuild the control the user is interacting with.
    pub(crate) fn filter_bar_enabled(&self) -> bool {
        !self.hide_filter_bar && !self.filters.is_empty()
    }
}

#[cfg(test)]
mod tests;
