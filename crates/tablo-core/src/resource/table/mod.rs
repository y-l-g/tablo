//! The [`Table`] builder plus query planning (`filter_expr`/`search_expr`/`order_bys_for`).
//!
//! Rendering lives in [`render`](self::render), CSV export in [`export`](self::export).
//! One routine applies the declaration for both loaders, and the essentials
//! check refuses a table whose page size cannot serve a list.

use std::{marker::PhantomData, sync::Arc};

use toasty::stmt::{Expr, List, OrderByExpr};
use topcoat::{Result, context::Cx};

use super::{
    column::{IntoColumns, TextColumn},
    filter::{Filter, IntoFilters},
    state::{TablePage, TableState},
};

mod export;
mod render;

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
pub struct RowActions {
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
pub struct GroupDef<M> {
    name: String,
    key: GroupKey<M>,
}

/// The action chrome a resource declares: which row actions
/// `wire_table_actions` attaches.
///
/// [`Resource::table`](crate::resource::Resource::table) returns a table
/// carrying no delete/edit/view prefix: the panel attaches them from the
/// resource's [`can_delete_any`](crate::resource::Resource::can_delete_any),
/// its record form, and its [`viewed`](crate::resource::Resource::viewed)
/// declaration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TableChrome {
    /// Whether the row renders a Delete action (which also enables bulk).
    pub(crate) delete: bool,
    /// Whether the row renders an Edit action.
    pub(crate) edit: bool,
    /// Whether the row renders a View action.
    pub(crate) view: bool,
}

/// A [`TableState`] whose `group_by` has already been checked against the
/// table's declared grouping.
///
/// [`Table::normalize_state`] is the only constructor, so a seam that takes
/// one reads the pre-normalized `group_by` directly and cannot normalize a
/// second time — or forget to normalize at all. The request entry
/// builds it once (the panel's list page, the `table_search` shard) and every
/// seam below takes this proof; the public render seams keep accepting a raw
/// `&TableState` and normalize it themselves, so an external page calling them
/// directly is unaffected.
///
/// Derefs to [`TableState`], so the seams that only read state keep their
/// `&TableState` signatures.
pub(crate) struct NormalizedState(TableState);

impl std::ops::Deref for NormalizedState {
    type Target = TableState;

    fn deref(&self) -> &TableState {
        &self.0
    }
}

/// How [`Table::order_bys_for`] falls back when `?sort=` names no sortable
/// column: the one axis the list loader and the CSV export
/// legitimately disagree on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderMode {
    /// List loading: the PK fallback applies only to a paginated
    /// table, where toasty's cursor pagination needs a deterministic order.
    List,
    /// CSV export: the chunked cursor walk needs a deterministic
    /// order whether or not the table paginates, so the PK fallback applies
    /// whenever no sortable column is declared.
    Export,
}

impl OrderMode {
    /// Whether the PK fallback applies, given whether the table paginates.
    fn falls_back_to_pk(self, paginated: bool) -> bool {
        match self {
            Self::List => paginated,
            Self::Export => true,
        }
    }
}

/// Table description of a `Resource`'s list view. Declares columns and how they
/// map to queries.
///
/// Row identity is mandatory and typed: [`Table::new`] takes the key
/// projection driving both key halves, and cells render via
/// [`TextColumn`]'s lens-bound closure where typos fail at compile time instead
/// of panicking at render.
pub struct Table<M> {
    columns: Vec<TextColumn<M>>,
    filters: Vec<Filter<M>>,
    group_by: Option<GroupDef<M>>,
    row_key: RowKey<M>,
    record_key: RowKey<M>,
    row_policy: Option<RowPolicy<M>>,
    page_size: Option<usize>,
    search_ui: Option<bool>,
    filters_ui: Option<bool>,
    delete_prefix: Option<String>,
    edit_prefix: Option<String>,
    view_prefix: Option<String>,
    bulk_delete: bool,
    live_search: bool,
    _marker: PhantomData<M>,
}

impl<M> std::fmt::Debug for Table<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Table")
            .field("columns", &self.columns)
            .field("filters", &self.filters.len())
            .field("group_by", &self.group_by.is_some())
            .field("row_policy", &self.row_policy.is_some())
            .field("page_size", &self.page_size)
            .field("search_ui", &self.search_ui)
            .field("filters_ui", &self.filters_ui)
            .field("delete_prefix", &self.delete_prefix)
            .field("edit_prefix", &self.edit_prefix)
            .field("view_prefix", &self.view_prefix)
            .field("bulk_delete", &self.bulk_delete)
            .field("live_search", &self.live_search)
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
    /// # Panics
    ///
    /// Panics on duplicate [`TextColumn::name`], the guard every constructor
    /// applies: sort resolution is
    /// first-sortable-`name()`-match, so duplicate sortable names would
    /// silently misresolve `?sort=`. The guard covers computed names too
    /// (`TextColumn::computed("Status", ..)` derives `name = "status"`) for
    /// namespace consistency and future-proofing. Same fail-loud policy as
    /// the GH #101 searchable/sortable panics and the Schema GH #100 guard.
    ///
    /// Also panics on an empty column set: a table with no columns renders a
    /// headers-only list, which no resource declares.
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
    /// # Panics
    ///
    /// Panics on duplicate [`TextColumn::name`] and on an empty column set,
    /// like [`Self::new`].
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

    /// The one constructor body: rejects duplicate column names, then builds
    /// the table around the two declared projections.
    fn from_keys(row_key: RowKey<M>, record_key: RowKey<M>, cols: impl IntoColumns<M>) -> Self
    where
        M: toasty::schema::Model,
    {
        let cols = cols.into_columns();
        assert!(
            !cols.is_empty(),
            "a Table needs at least one column: declare columns with Table::new(key, columns)"
        );
        let mut seen = std::collections::HashSet::with_capacity(cols.len());
        for c in &cols {
            let name = c.name();
            assert!(
                seen.insert(name),
                "duplicate column name '{name}': each Table column needs a distinct name (GH #156)"
            );
        }
        Self {
            columns: cols,
            filters: Vec::new(),
            group_by: None,
            row_key,
            record_key,
            row_policy: None,
            page_size: None,
            search_ui: None,
            filters_ui: None,
            delete_prefix: None,
            edit_prefix: None,
            view_prefix: None,
            bulk_delete: false,
            live_search: false,
            _marker: PhantomData,
        }
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
    /// Panics on duplicate [`Filter::name`], the same fail-loud
    /// policy as [`Self::new`]: the `filters` transport is one
    /// `name:value` pair per declared filter, and `parse_filters_param` keeps
    /// the first value for a duplicated key, so two filters sharing a name
    /// would silently drop one of them.
    pub fn filters(mut self, filters: impl IntoFilters<M>) -> Self
    where
        M: toasty::schema::Model,
    {
        let filters = filters.into_filters();
        let mut seen = std::collections::HashSet::with_capacity(filters.len());
        for f in &filters {
            let name = f.name();
            assert!(
                seen.insert(name),
                "duplicate filter name '{name}': each Table filter needs a distinct name (GH #294)"
            );
        }
        self.filters = filters;
        self
    }

    /// The relations this table's columns declared their projections read,
    /// merged into one set.
    ///
    /// The export hands this to
    /// [`Resource::export_query`](super::Resource::export_query), which is
    /// the only thing that can turn a name into a typed `include(..)`. Every
    /// column is rendered — the CSV writes a cell per column — so the set is
    /// the union over all of them; a table cannot narrow its export by
    /// declaring fewer needs than its columns read, because the reading
    /// column is the one that renders.
    pub fn include_needs(&self) -> super::IncludeNeeds
    where
        M: toasty::schema::Model,
    {
        self.columns
            .iter()
            .flat_map(|c| c.include_names())
            .copied()
            .collect()
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
    /// An oversized `?filters=` transport arrives here as
    /// `FILTERS_OVERFLOW_SEGMENT`, reported with its own reason so
    /// the warning says the transport was refused rather than misdescribing it
    /// as malformed.
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
        for segment in &state.malformed_filters {
            let reason = if segment == super::state::FILTERS_OVERFLOW_SEGMENT {
                "too many filters: refused whole (GH #205)"
            } else {
                "malformed: expected key:value"
            };
            out.push((segment.clone(), reason.to_string()));
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
    /// In live tables the page-load value seeds the `group_by` interaction
    /// signal and persists across in-place reruns; changing it is
    /// still a navigation (`?group_by=` links) until a live control ships.
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
    /// The request entry normalizes **once** and every render seam below takes
    /// the proof ([`NormalizedState`]): the panel's list page and the
    /// `table_search` shard normalize at the point they parse the state, and
    /// pass the result down, so a live list request normalizes once instead of
    /// once per seam. The public seams still normalize their own
    /// `&TableState` argument, so a page calling them directly keeps the GH
    /// #153 guarantee without knowing about this type.
    pub(crate) fn normalize_state(&self, state: &TableState) -> NormalizedState {
        let mut out = state.clone();
        if self.group_by.as_ref().map(|def| def.name.as_str()) != out.group_by.as_deref() {
            out.group_by = None;
        }
        NormalizedState(out)
    }

    /// Enable real cursor pagination with the given page size.
    ///
    /// Loaders pair this with toasty's `.paginate(per_page)` (via
    /// [`TablePage::from_toasty_page`]); the render then shows Previous/Next
    /// links built from the executed page's cursors — never fake page
    /// numbers. Also implies a deterministic PK ordering when the table
    /// declares no sortable column (see [`Self::order_bys_for`]).
    ///
    /// A zero page size is a programmer error: it fails loudly at render/load
    /// time with a descriptive error, never a bare panic.
    pub fn paginate(mut self, per_page: usize) -> Self {
        self.page_size = Some(per_page);
        self
    }

    /// Whether the page size was declared via [`Self::paginate`].
    pub fn page_size(&self) -> Option<usize> {
        self.page_size
    }

    /// Which row actions `record` allows: the panel-wired policy, or
    /// [`RowActions::ALL`] when the table declares none.
    ///
    /// The renderer reads this per row to decide the View/Edit/Delete links and
    /// whether the bulk checkbox is enabled.
    pub fn actions_for(&self, record: &M) -> RowActions {
        self.row_policy
            .as_ref()
            .map_or(RowActions::ALL, |policy| policy(record))
    }

    /// Force the search toolbar on or off.
    ///
    /// Defaults to showing the toolbar whenever at least one column is
    /// `searchable()`, so the toolbar and the query stay in step.
    pub fn search(mut self, enabled: bool) -> Self {
        self.search_ui = Some(enabled);
        self
    }

    /// Force the filter bar on or off.
    ///
    /// Defaults to showing the bar whenever the table declares filters. The
    /// live list hoists the bar out of the swapped table and turns it off here
    /// mirroring how `search(false)` hands the search toolbar to the
    /// page: a `<select>` that is rebuilt by its own rerun loses focus and
    /// collapses its native popup.
    pub fn filter_bar(mut self, enabled: bool) -> Self {
        self.filters_ui = Some(enabled);
        self
    }

    /// Keystroke-live search via the `table_search` shard.
    ///
    /// When enabled, the toolbar renders a signal-backed input that
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
    pub fn live_search(mut self, enabled: bool) -> Self {
        self.live_search = enabled;
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

    /// Whether the bulk checkbox column renders: bulk selection plus a delete
    /// prefix to post to.
    fn bulk_enabled(&self) -> bool {
        self.bulk_delete && self.delete_prefix.is_some()
    }

    /// Global search predicate — OR across searchable columns.
    ///
    /// Substring match (`?q=` anywhere in the value), escaped so a term
    /// containing `%` or `_` stays literal; see
    /// [`TextColumn::to_search_expr`](crate::resource::TextColumn::to_search_expr)
    /// for the driver case-sensitivity caveat.
    pub fn search_expr(&self, term: &str) -> Option<Expr<bool>>
    where
        M: toasty::schema::Model,
    {
        let t = term.trim();
        if t.is_empty() {
            return None;
        }
        let mut exprs = self.columns.iter().filter_map(|c| c.to_search_expr(t));
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
        self.columns.iter().find_map(|c| c.to_order_by(descending))
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
    /// 3. otherwise, when `mode` asks for it, the PK alone — cursor pagination requires a
    ///    deterministic order even with no sortable column, and toasty only *extends* an existing
    ///    non-empty ordering.
    ///
    /// Loaders that also need the search term parse the state once with
    /// [`TableState::from_cx`] and apply the declaration through
    /// `Self::apply_declaration` (see `crate::panel::Panel`'s generic
    /// resource list handler).
    pub fn order_bys_for(&self, state: &TableState, mode: OrderMode) -> Vec<OrderByExpr>
    where
        M: toasty::schema::Model,
    {
        if let Some(sort) = &state.sort
            && let Some(col) = self
                .columns
                .iter()
                .find(|c| c.is_sortable() && c.name() == sort.column)
            && let Some(ord) = col.to_order_by(sort.descending)
        {
            return vec![ord];
        }
        let out: Vec<OrderByExpr> = self.order_by(false).into_iter().collect();
        if out.is_empty() && mode.falls_back_to_pk(self.page_size.is_some()) {
            return Self::pk_order_bys();
        }
        out
    }

    /// Apply this table's declaration to `query` — the one routine that turns
    /// the search term, the filters and the ordering into a query.
    ///
    /// `query` is the caller's seed, which is the one thing the two loaders
    /// legitimately differ on: the list loads the tenant-scoped
    /// [`Resource::query`](crate::resource::Resource::query) (the row-scoping
    /// seam, ADR-0002) while the export loads the tenant-scoped
    /// [`Resource::export_query`](crate::resource::Resource::export_query),
    /// narrowed to the relations the rendered columns declared.
    /// `mode` picks the ordering fallback each caller needs.
    ///
    /// Everything else is shared, so a new search or filter dimension cannot
    /// reach the list and miss the CSV — the drift class GH #172 fixed.
    pub(crate) fn apply_declaration(
        &self,
        mut query: toasty::stmt::Query<List<M>>,
        state: &TableState,
        mode: OrderMode,
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
        for ord in self.order_bys_for(state, mode) {
            query = query.order_by(ord);
        }
        query
    }

    /// Resolve and execute this table's query for `state` — search, filters,
    /// ordering, and cursor pagination — and return the rows.
    ///
    /// The loader half of the live-table seam (GH #154 §2): a page that owns
    /// its own table can hand its shard a query and this
    /// hook applies the same declaration pipeline `panel::load_table_page`
    /// applies to the tenant-scoped `Resource::query`, so a
    /// page-level shard does not
    /// reimplement filtering, ordering, or cursor validation. The table such a
    /// page serves comes from
    /// [`panel::wired_table`](crate::panel::wired_table), which carries the
    /// resource's action chrome.
    ///
    /// The cursor-existence probes reuse `query`, so they pay the query's
    /// relation includes. The panel's resource-list loader seeds them from a
    /// narrower query with the same scope and no includes; this entry point has
    /// no such seed, so its probes carry the query's includes.
    pub async fn load(
        &self,
        cx: &Cx,
        query: toasty::stmt::Query<List<M>>,
        state: &TableState,
    ) -> Result<TablePage<M>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        self.load_with_probe(cx, query.clone(), query, state).await
    }

    /// [`Self::load`] with a separate seed for the cursor-existence probes.
    /// The probes only ask whether one more row exists past a cursor, so they
    /// read no relation and do not need `query`'s includes. `probe_query` is
    /// the same scope and declaration pipeline with those includes dropped;
    /// passing `query` itself reproduces [`Self::load`].
    pub(crate) async fn load_with_probe(
        &self,
        cx: &Cx,
        query: toasty::stmt::Query<List<M>>,
        probe_query: toasty::stmt::Query<List<M>>,
        state: &TableState,
    ) -> Result<TablePage<M>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        if self.page_size == Some(0) {
            return Err(std::io::Error::other(
                "Table::load: paginate requires per_page > 0 (GH #96)",
            )
            .into());
        }
        // The declaration becomes predicates and an ordering through the one
        // shared routine — the export loader applies the same one to
        // its own seed query.
        let query = self.apply_declaration(query, state, OrderMode::List);
        let mut db = crate::db::db(cx);
        match self.page_size {
            Some(per_page) => {
                // Keep a cursor-free copy of the filtered+ordered probe seed for
                // cursor validation: Toasty's `Page` sets `next_cursor`
                // optimistically whenever `len == page_size`, which leaves a
                // phantom cursor when the page sits exactly at a boundary. The
                // probe seed carries no relation includes because the
                // probes only ask whether a row exists.
                let base_query = self.apply_declaration(probe_query, state, OrderMode::List);
                let mut paginated = toasty::stmt::Paginate::new(query, per_page);
                // Toasty cursor pagination takes exactly one cursor:
                // a URL carrying both `?after=` and `?before=` must fail loudly
                // instead of silently preferring `after` (the GH #93 fail-open
                // family). The `CursorDecodeError` marker gives the failure the
                // drop-pagination retry contract.
                if state.after.is_some() && state.before.is_some() {
                    return Err(crate::cursor::CursorDecodeError::conflicting_cursors());
                }
                if let Some(cursor) = &state.after {
                    paginated = paginated.after(crate::cursor::decode(cursor)?);
                } else if let Some(cursor) = &state.before {
                    paginated = paginated.before(crate::cursor::decode(cursor)?);
                }
                let loaded = paginated
                    .exec(&mut db)
                    .await
                    .map_err(|error| reject_cursor(error.into(), state))?;
                let mut page = TablePage::from_toasty_page(loaded)?;
                // Cursor-existence probes, one per landing direction:
                // the engine sets `next_cursor`/`prev_cursor` optimistically,
                // so a page sitting exactly at a boundary carries a phantom
                // cursor without validation. Each direction probes only the
                // edge that can lie:
                // - forward/first landing: prev is exact (absent on the first page; otherwise the
                //   page we came from exists), next may be phantom at the end boundary → probe next
                //   on full pages. A short page cannot have a next page.
                // - backward landing: next is exact (the page we came from follows), prev may be
                //   phantom when the fetch lands on the first page → probe prev whenever one is
                //   reported.
                //
                // Deliberately NOT a `LIMIT per_page+1` fold: the engine
                // derives `next_cursor` from the last *fetched* row, so
                // trimming the extra row would anchor the next link past it —
                // every `(per_page+1)`th row would vanish from forward walks.
                // The probes keep the main fetch's cursors (which point at
                // displayed rows) as the link anchors.
                //
                // Residual (same as ever): a concurrent delete landing between
                // the main fetch and the click can still void a validated
                // cursor — that degrades to the void-window recovery link
                // never to silently skipped rows.
                if state.before.is_some() {
                    if let Some(cursor) = page.prev_cursor.clone() {
                        let probe = toasty::stmt::Paginate::new(base_query, 1)
                            .before(crate::cursor::decode(&cursor)?)
                            .exec(&mut db)
                            .await
                            .map_err(topcoat::Error::from)?;
                        if probe.items.is_empty() {
                            page.prev_cursor = None;
                        }
                    }
                } else if page.rows.len() == per_page {
                    if let Some(cursor) = page.next_cursor.clone() {
                        let probe = toasty::stmt::Paginate::new(base_query, 1)
                            .after(crate::cursor::decode(&cursor)?)
                            .exec(&mut db)
                            .await
                            .map_err(topcoat::Error::from)?;
                        if probe.items.is_empty() {
                            page.next_cursor = None;
                        }
                    }
                } else {
                    // Short page → no next, keep prev as-is (has_previous already correct).
                    page.next_cursor = None;
                }
                Ok(page)
            }
            None => {
                let rows: Vec<M> = query.exec(&mut db).await.map_err(topcoat::Error::from)?;
                Ok(rows.into())
            }
        }
    }

    /// The first declaration this table is missing, if any.
    ///
    /// The same check [`Self::render`](Self::render_with_state) enforces per
    /// request, lifted so [`Panel::build`](crate::panel::Panel::build) can
    /// refuse to serve a resource whose table could never render — the
    /// declaration is knowable at boot, so a request is too late to report it.
    pub(crate) fn missing_essentials(&self) -> Option<String> {
        if self.page_size == Some(0) {
            return Some("paginate requires per_page > 0".to_string());
        }
        None
    }

    /// Whether the search toolbar renders: the explicit `search(bool)` value,
    /// or auto — at least one `searchable()` column.
    pub(crate) fn search_enabled(&self) -> bool
    where
        M: toasty::schema::Model,
    {
        self.search_ui
            .unwrap_or_else(|| self.columns.iter().any(|c| c.is_searchable()))
    }

    /// Whether this table renders the keystroke-live search host.
    pub(crate) fn is_live_search(&self) -> bool {
        self.live_search
    }

    /// Whether the filter bar renders inside the table: the explicit
    /// `filter_bar(bool)` value, or auto — the table declares at least one filter.
    ///
    /// Live tables turn it off: the list page hoists the bar out of
    /// the swapped region, the same way it owns the search toolbar, so a filter
    /// change cannot rebuild the control the user is interacting with.
    pub(crate) fn filter_bar_enabled(&self) -> bool {
        self.filters_ui.unwrap_or(!self.filters.is_empty())
    }
}

/// Attribute a failed paginated fetch to the request's cursor.
///
/// A token cut from a different ordering decodes but the engine refuses the
/// statement (`invalid_statement`: its field count no longer matches the
/// query's `ORDER BY`). No other statement this paginated loader builds carries
/// that error while the request names a cursor. Such a failure is the cursor's,
/// so it takes the cursor-stripped retry contract instead of re-requesting the
/// identical URL forever; every other failure keeps the cursor.
fn reject_cursor(error: topcoat::Error, state: &TableState) -> topcoat::Error {
    let cursored = state.after.is_some() || state.before.is_some();
    let rejected = error
        .downcast_ref::<toasty::Error>()
        .is_some_and(toasty::Error::is_invalid_statement);
    if cursored && rejected {
        crate::cursor::CursorRejectedError::rejected(&error)
    } else {
        error
    }
}

#[cfg(test)]
mod tests;
