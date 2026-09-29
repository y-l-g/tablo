//! Static and live entry points, row assembly, thead, and column widths.

use std::borrow::Cow;

use tablo_ui::{
    ButtonSize, ButtonVariant, button_variants, icons, table, table_body, table_cell, table_head,
    table_header, table_row,
};
use topcoat::{Result, context::Cx, icon::icon, runtime::Event, view::*};

use super::super::{
    super::{
        ColumnWidth,
        column::NARROW_DEFAULT_PERCENT,
        state::{
            TablePage, TableSignals, TableState, delete_action_url, group_header_dom_id,
            row_dom_id, row_edit_url, row_view_url,
        },
    },
    GroupKey, NormalizedState, RowActions, RowKey, Table,
};

/// The readability floor one [`ColumnWidth::Wide`](super::super::ColumnWidth::Wide)
/// column contributes to the table's `min-width`, in whole rem.
///
/// A wide column declares no width — it takes what the declared columns leave —
/// so a sum of declared widths alone would let it crush to zero on a narrow
/// viewport (the measured 38px cells at 480px). Six rem keeps body text
/// readable and, summed across the wide columns, trips the wrapper's
/// horizontal scroll before the fixed layout crushes them.
const WIDE_COLUMN_MIN_REM: u8 = 6;

/// The share of the table the bulk-selection column claims: one
/// checkbox plus the cell's `p-3` padding at the widths a list is read at. A
/// percentage, not a length: the column keeps its share as the table narrows,
/// and the columns that declare none keep theirs.
const BULK_COLUMN_PERCENT: u8 = 5;

/// The most of the table the kind defaults claim together.
///
/// The defaults are shares of the table, and the columns that declare none
/// take what they leave: a total over 100% gives those columns no space at
/// all, and `table-fixed` renders a column with no space at zero width, header
/// text included. The budget keeps the rest of the table for them whatever the
/// column set.
const DEFAULT_WIDTH_BUDGET_PERCENT: u8 = 60;

/// The share a kind default claims, scaled down when the table's defaults
/// together exceed [`DEFAULT_WIDTH_BUDGET_PERCENT`].
fn scaled_default_percent(nominal: u8, total: u32) -> u8 {
    if total <= u32::from(DEFAULT_WIDTH_BUDGET_PERCENT) {
        return nominal;
    }
    let scaled = u32::from(nominal) * u32::from(DEFAULT_WIDTH_BUDGET_PERCENT) / total;
    // `scaled` is at most the budget, so the conversion cannot fail.
    u8::try_from(scaled).unwrap_or(DEFAULT_WIDTH_BUDGET_PERCENT)
}

/// The `style` value a kind default emits.
fn default_width_style(percent: u8) -> Cow<'static, str> {
    Cow::Owned(format!("width: {percent}%"))
}

/// The width every column of one render declares: one `style` value
/// per declared column, in column order, plus the two chrome columns.
/// `None` is a column that declares no width — a wide column, which takes a
/// share of what the declared ones leave.
///
/// `actions_min` is the actions column's content floor for its body cells
/// (the header carries the share *and* the floor); `table_min_width` is the
/// table-level floor — the sum of the declared widths — that lets the
/// wrapper's `overflow-x-auto` scroll on a narrow viewport instead of
/// crushing the cells.
pub(super) struct ColumnWidths {
    cells: Vec<Option<Cow<'static, str>>>,
    bulk: Option<Cow<'static, str>>,
    actions: Option<Cow<'static, str>>,
    actions_min: Option<Cow<'static, str>>,
    pub(super) table_min_width: Option<Cow<'static, str>>,
}

impl<M> Table<M> {
    /// Render the table for the given loaded page.
    ///
    /// Real chrome, no fake affordances: the header renders sort **links**
    /// driving `?sort=`/`?dir=` and a search toolbar driving `?q=` (shown by
    /// default when any column is `searchable()`), rows are keyed by the
    /// projection declared via [`Table::new`](super::super::Table::new) per
    /// `CONTEXT.md` and rendered via
    /// each column's typed projection, pagination shows Previous/Next links
    /// built from the executed page's **real** cursors (never invented page
    /// numbers), and the empty state reflects whether a search was active.
    ///
    /// Composes the synced `tablo-ui` primitives and Token classes
    /// (`border-border` on the chrome, `bg-background`/`shadow-xs` on the
    /// toolbar controls, `text-muted-foreground` on the captions) — no raw
    /// colors, no `ac-*`.
    ///
    /// # Errors
    ///
    /// Errors when pagination declares `per_page = 0`.
    pub async fn render<'a>(&self, cx: &'a Cx, page: TablePage<M>) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        let state = TableState::from_cx(cx);
        let path = topcoat::context::try_request_context::<http::request::Parts>(cx)
            .map(|parts| parts.uri.path().to_string())
            .unwrap_or_default();
        self.render_with_state(cx, page, &state, &path).await
    }

    /// Render with explicit list state and path instead of reading them from
    /// `cx` — the seam a live-search shard needs: shard requests hit
    /// the `table_search` shard's own endpoint, so `TableState::from_cx` would
    /// see the endpoint URI, not the list page's `?q=/filters/sort`. Callers
    /// pass the page's state (or shard args rebuilt via
    /// [`TableSignals::to_state`]) and the list URL explicitly.
    ///
    /// Normalizes the state it is handed, so a page calling this
    /// directly needs no knowledge of `NormalizedState`; a caller that
    /// already normalized once per request goes through
    /// `Self::render_normalized` instead.
    pub async fn render_with_state<'a>(
        &self,
        cx: &'a Cx,
        page: TablePage<M>,
        state: &TableState,
        path: &str,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        self.render_normalized(cx, page, &self.normalize_state(state), path)
            .await
    }

    /// [`Self::render_with_state`] with the state already normalized
    /// the request entry normalizes once and every seam below takes
    /// the proof, so a live list request never normalizes the same state
    /// twice.
    pub(crate) async fn render_normalized<'a>(
        &self,
        cx: &'a Cx,
        page: TablePage<M>,
        state: &NormalizedState,
        path: &str,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        self.render_inner(cx, page, state, path, None).await
    }

    /// Render the interactive body for a live table: the same
    /// presentation as [`Self::render_with_state`], with the sort links, the
    /// pager, the filter transport, and the empty-state clear links bound to
    /// `signals` — each interaction writes a signal and the browser morphs the
    /// shard's new output in place, without a navigation or a scroll jump.
    /// Every bound control keeps its real `href`/form, so a page without JS
    /// still navigates as before.
    ///
    /// The row-delete dialog is not part of this output: it lives
    /// outside the region a rerun swaps, rendered once by the page that owns
    /// the signals, so a caller rendering only through this method renders
    /// [`Self::render_delete_dialog`] itself to keep the `?delete=` fallback.
    pub async fn render_live_with_state<'a>(
        &self,
        cx: &'a Cx,
        page: TablePage<M>,
        state: &TableState,
        path: &str,
        signals: TableSignals,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        self.render_live_normalized(cx, page, &self.normalize_state(state), path, signals)
            .await
    }

    /// [`Self::render_live_with_state`] with the state already normalized
    /// the `table_search` shard normalizes once and renders
    /// through here.
    pub(crate) async fn render_live_normalized<'a>(
        &self,
        cx: &'a Cx,
        page: TablePage<M>,
        state: &NormalizedState,
        path: &str,
        signals: TableSignals,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        self.render_inner(cx, page, state, path, Some(signals))
            .await
    }

    async fn render_inner<'a>(
        &self,
        cx: &'a Cx,
        page: TablePage<M>,
        state: &NormalizedState,
        path: &str,
        signals: Option<TableSignals>,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        if self.page_size == Some(0) {
            return Err(std::io::Error::other(
                "Table::render: paginate requires per_page > 0 (GH #96)",
            )
            .into());
        }
        let row_key = self.row_key.clone();
        let delete_prefix = self.delete_prefix.clone();
        let with_actions = self.with_actions();
        let with_bulk = self.bulk_enabled();
        let record_key = self.record_key.clone();
        let head = self
            .render_thead(cx, state, path, with_actions, with_bulk, signals.as_ref())
            .await?;
        let show_search = self.search_enabled();
        let search_bar = if show_search {
            Some(self.render_search_bar(cx, state, path).await?)
        } else {
            None
        };
        let show_filters = self.filter_bar_enabled();
        let filter_bar = if show_filters {
            Some(
                self.render_filter_bar(cx, state, path, signals.as_ref())
                    .await?,
            )
        } else {
            None
        };
        let bulk_bar_view = self.render_bulk_bar(cx, signals.as_ref());
        let pager = self
            .render_pager(cx, state, path, &page, signals.as_ref())
            .await?;
        let filter_warning = self.render_filter_warning(cx, state, path);
        // The declared grouping, when `?group_by=` names it. Read
        // before the row projection so each row can carry its group label,
        // which the page-local shim orders by.
        let group_key = self.effective_group_key(state);
        let row_data = self.row_views(
            state,
            path,
            &page,
            &row_key,
            &record_key,
            group_key.as_ref(),
        );
        // The confirmation dialog lives with the delete chrome and
        // ships closed, so a row control opens it in place. A live
        // output carries none: the page that owns the signals renders it once,
        // outside the region a rerun swaps (`panel::resource_list_live`). The
        // dialog renders on every page with delete chrome, not only under
        // `?delete=`, so without this gate every shard rerun would morph a
        // second copy — and its ids — into the page.
        let delete_dialog = if signals.is_none() {
            self.render_delete_dialog_normalized(cx, state).await?
        } else {
            None
        };

        // Body-only branch: the empty and rows pages share the one
        // chrome wrapper built below — only the table body differs. Group
        // headers and the pager exist solely on rows pages: an empty page
        // renders the honest empty cell instead (its pager would be empty
        // anyway, and grouping an empty page yields no headers).
        //
        // The declared widths are a property of the columns, not of the row,
        // so they are resolved once here: the same CSS for every row, and a
        // `for` whose expression names `self.columns` would carry the table's
        // borrow into the view.
        let ColumnWidths {
            cells: cell_widths,
            actions_min,
            table_min_width,
            ..
        } = self.column_widths();
        let mut pager_views: Vec<BoxView<'_>> = Vec::new();
        let body: BoxView<'_> = if page.rows.is_empty() {
            let empty_cell = self
                .render_empty_cell(cx, state, path, with_actions, with_bulk, signals.as_ref())
                .await?;
            view! {
                cx =>
                table(
                    attrs: attributes! { class="table-fixed" style=(table_min_width.as_deref()) },
                    (head)
                    (empty_cell)
                )
            }
            .boxed()
        } else {
            pager_views = pager;
            // One table body for grouped and ungrouped pages: each grouped
            // row carries the header its group's first row owns, so
            // the header lands inside the table immediately above its own
            // rows instead of a count legend stacked over an ungrouped table.
            let header_colspan =
                self.columns.len() + usize::from(with_bulk) + usize::from(with_actions);
            // The one dialog every row control on this table opens;
            // empty without delete chrome, where no control renders one.
            let delete_dialog_id = delete_prefix
                .as_deref()
                .map(Self::delete_dialog_dom_id)
                .unwrap_or_default();
            view! {
                cx =>
                // The table-level layout is a static class: Tailwind sees the
                // literal, and the decision carries no per-column value. Each
                // column's width, which does, rides the `th`/`td` inline
                // `style`. Fixed layout is what stops a filter or a
                // page change from re-measuring the columns. The `min-width`
                // is the sum of those declared widths: with `w-full` the
                // table never exceeds its container on its own, so without
                // the floor the wrapper's `overflow-x-auto` never scrolls.
                table(
                    attrs: attributes! { class="table-fixed" style=(table_min_width.as_deref()) },
                    (head)
                    table_body(
                        #[key(row.key.as_str())]
                        for row in &row_data {
                            let key_for_row = row.key.clone();
                            let key_for_select = row.record_id.clone();
                            let view_for_row = row.view_url.clone();
                            let edit_for_row = row.edit_url.clone();
                            let open_for_row = row.delete_url.clone();
                            let delete_action_for_row = row.delete_action.clone();
                            let delete_dialog_for_row = delete_dialog_id.clone();
                            let selectable_for_row = row.selectable;
                            let row_dom_id = row_dom_id(&key_for_row);
                            if let Some(header) = row.group_header.clone() {
                                table_row(
                                    attrs: attributes! { id=(header.dom_id) },
                                    table_cell(
                                        attrs: attributes! {
                                            colspan=(header_colspan)
                                            class="px-4 py-2 bg-muted text-sm font-medium"
                                        },
                                        (header.text)
                                    )
                                )
                            }
                            table_row(
                                attrs: attributes! { id=(row_dom_id) },
                                if with_bulk {
                                    if selectable_for_row {
                                        table_cell(
                                            <input
                                                type="checkbox"
                                                value=(key_for_select)
                                                aria-label="Select row"
                                                data-row-select=""
                                            >
                                        )
                                    } else {
                                        // A refused row renders no checkbox:
                                        // selecting it could only produce a
                                        // batch the handler refuses. The cell
                                        // stays so the row keeps its shape.
                                        table_cell()
                                    }
                                }
                                // `row.cells` is built column-for-column, so the
                                // zip pairs each cell with the column that owns
                                // its width. The cell repeats the
                                // width its header declares and truncates:
                                // under the table's fixed layout a value wider
                                // than the column clips to an ellipsis instead
                                // of stretching the column. `truncate` is a
                                // static class, which Tailwind does generate —
                                // only the per-column width has to be data.
                                for (cell, width) in row.cells.iter().zip(&cell_widths) {
                                    table_cell(
                                        attrs: attributes! { class="truncate" style=(width.as_deref()) },
                                        (cell.clone())
                                    )
                                }
                                // Every row carries the actions cell its header
                                // declares; a row refused every link keeps an
                                // empty cell so the row keeps its shape.
                                if with_actions {
                                    table_cell(
                                        attrs: attributes! { style=(actions_min.as_deref()) },
                                        <div class="flex gap-2">
                                            if let Some(url) = view_for_row {
                                                <a
                                                    href=(url)
                                                    class=(button_variants(
                                                        ButtonVariant::Outline,
                                                        ButtonSize::Md,
                                                    ))
                                                >
                                                    "View"
                                                </a>
                                            }
                                            if let Some(url) = edit_for_row {
                                                <a
                                                    href=(url)
                                                    class=(button_variants(
                                                        ButtonVariant::Outline,
                                                        ButtonSize::Md,
                                                    ))
                                                >
                                                    "Edit"
                                                </a>
                                            }
                                            if let (Some(url), Some(action)) = (
                                                open_for_row,
                                                delete_action_for_row,
                                            ) {
                                                <a
                                                    href=(url)
                                                    data-row-delete-trigger=(delete_dialog_for_row)
                                                    data-row-delete-action=(action)
                                                    class=(button_variants(
                                                        ButtonVariant::Destructive,
                                                        ButtonSize::Md,
                                                    ))
                                                >
                                                    "Delete"
                                                </a>
                                            }
                                        </div>
                                    )
                                }
                            )
                        }
                    )
                )
            }
            .boxed()
        };

        // The refresh control: a live table's region re-renders when
        // a signal its shard tracked changes, and a mutation changes rows the
        // tracked inputs do not describe — the query is the same, the data is
        // not. One write that means "re-read the table" is therefore the
        // mutation's only honest in-place effect: the client bumps this
        // revision token and the shard re-runs the query, morphing and
        // re-hydrating the region through the seam that already exists.
        //
        // The signal is declared here, inside the shard's own output, so it
        // belongs to the shard's content scope: its id derives from the shard
        // invocation's identity and this call site, so it is stable across
        // reruns and distinct per shard, and the runtime keeps its value when
        // the declaration renders again. Reading it for the input's initial
        // value is what declares the dependency; the token itself is opaque.
        //
        // A static table renders no control at all: it has no shard to re-run,
        // its region is inert markup, and the client replaces it wholesale
        // So the control's presence *is* the page's answer to
        // "can this table refresh in place?".
        let revision_attrs = signals.as_ref().map(|_| {
            let revision = topcoat::runtime::signal(cx, || "0".to_string());
            attributes! {
                cx =>
                type="hidden"
                value=(revision.get())
                :value=$(revision.get())
                @change=$(|e: Event| revision.set(e.target.value))
                data-table-revision=""
            }
        });

        // One chrome wrapper for both branches: search bar, filter bar, bulk
        // bar, warning, table body, pager, dialog, inside the
        // `data-boundary` region the morph swaps.
        let inner = view! {
            cx =>
            <div
                class="rounded-xl border border-border overflow-hidden"
                data-table-root=""
            >
                if show_search {
                    (search_bar.expect("search bar built when enabled"))
                }
                if show_filters {
                    (filter_bar.expect("filter bar built when enabled"))
                }
                (bulk_bar_view)
                if let Some(attrs) = revision_attrs {
                    <input (attrs)>
                }
                if let Some(warning) = filter_warning {
                    (warning)
                }
                (body)
                for p in pager_views {
                    (p)
                }
                if let Some(dialog) = delete_dialog {
                    (dialog)
                }
            </div>
        };
        Ok(view! { cx => <div data-boundary="table">(inner)</div> }.boxed())
    }

    /// Project the loaded page into the row presentation the template renders.
    ///
    /// Precomputed so template bodies capture only owned data — the lazy view
    /// outlives the render call, so it must never borrow `self` or `page`.
    ///
    /// The per-row delete URL opens the confirmation dialog on the list page
    /// (`?delete=<key>`); the per-row edit URL links to `{prefix}/{key}/edit`.
    /// Both — and the bulk checkbox values — carry the *record* key, resolved by
    /// handlers as the model's typed PK; the display `key` stays on keyed diffs
    /// and DOM ids.
    ///
    /// The chrome prefixes say which links the table *can* render; the
    /// [`Table::row_actions`] policy says which of them *this* record may use.
    /// A denied action emits no URL, and a row denied `delete` renders no bulk
    /// checkbox. The policy is consulted only when a prefix is wired.
    ///
    /// The delete URL's shared parameters are encoded once for the whole page,
    /// because the filter transport is the expensive half and rebuilding it per
    /// row is work a client can inflate with one oversized `?filters=`.
    ///
    /// Page-local grouping is display-only: `group_by` is a bare key closure with
    /// no lens, so no `ORDER BY` is derivable and a group cannot span pages. The
    /// shim therefore reorders *this page's* rows by the group label — a stable
    /// sort, so rows keep the query's order inside their group — and hangs each
    /// group's header off its first row. The query, its cursors and the export
    /// keep the declared ordering.
    ///
    /// Row keys must be injective within a page: duplicates corrupt keyed diffs
    /// and bulk selection.
    fn row_views(
        &self,
        state: &NormalizedState,
        path: &str,
        page: &TablePage<M>,
        row_key: &RowKey<M>,
        record_key: &RowKey<M>,
        group_key: Option<&GroupKey<M>>,
    ) -> Vec<RowView>
    where
        M: toasty::schema::Model,
    {
        let delete_url_base = self
            .delete_prefix
            .as_ref()
            .map(|_| state.row_url_base(path));
        let gated = self.delete_prefix.is_some()
            || self.edit_prefix.is_some()
            || self.view_prefix.is_some();
        let mut row_data: Vec<RowView> = page
            .rows
            .iter()
            .map(|row| {
                let key = row_key(row);
                let record_id = record_key(row);
                let actions = if gated {
                    self.actions_for(row)
                } else {
                    RowActions::ALL
                };
                let cells: Vec<String> = self
                    .columns
                    .iter()
                    .map(|col| col.render_cell(row))
                    .collect();
                let edit_url = self
                    .edit_prefix
                    .as_ref()
                    .filter(|_| actions.edit)
                    .map(|prefix| row_edit_url(prefix, &record_id));
                let view_url = self
                    .view_prefix
                    .as_ref()
                    .filter(|_| actions.view)
                    .map(|prefix| row_view_url(prefix, &record_id));
                let delete_url = delete_url_base
                    .as_ref()
                    .filter(|_| actions.delete)
                    .map(|base| base.delete_dialog(&record_id));
                // The shared dialog's POST target for this row: the
                // row control hands it over before opening the dialog, so the
                // action and the control come from the one policy decision.
                let delete_action = self
                    .delete_prefix
                    .as_ref()
                    .filter(|_| actions.delete)
                    .map(|prefix| delete_action_url(prefix, &record_id));
                RowView {
                    key,
                    record_id,
                    cells,
                    view_url,
                    edit_url,
                    delete_url,
                    delete_action,
                    selectable: actions.delete,
                    group: group_key.map(|group| group(row)),
                    group_header: None,
                }
            })
            .collect();
        if group_key.is_some() {
            row_data.sort_by(|a, b| a.group.cmp(&b.group));
            let mut start = 0;
            while start < row_data.len() {
                let label = row_data[start].group.clone().unwrap_or_default();
                let mut end = start;
                while end < row_data.len() && row_data[end].group.as_deref() == Some(label.as_str())
                {
                    end += 1;
                }
                // The count is page-local, and says so: a group split across
                // pages must not read as a table total. The header
                // carries an id derived from its label — never from its
                // position — so the in-place morph can follow it.
                row_data[start].group_header = Some(GroupHeader {
                    dom_id: group_header_dom_id(&label),
                    text: format!("{label} ({} on this page)", end - start),
                });
                start = end;
            }
        }
        debug_assert!(
            {
                let mut seen = std::collections::HashSet::new();
                row_data.iter().all(|row| seen.insert(row.key.clone()))
            },
            "duplicate table keys in one page: the key projection must be injective"
        );
        row_data
    }

    /// The zero-rows cell — one honest message, not two: "no records yet"
    /// when unfiltered, "no results" with a Clear link when a search is
    /// active. The dead Create button is gone (create pages are not wired
    /// yet). Wrapped in a single cell spanning the table so it sits inside
    /// the table. For live tables (`signals`) the clear/back links write the
    /// signals instead of navigating; `href` stays the fallback.
    async fn render_empty_cell<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        with_actions: bool,
        with_bulk: bool,
        signals: Option<&TableSignals>,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model,
    {
        let mut colspan = self.columns.len();
        if with_bulk {
            colspan += 1;
        }
        if with_actions {
            colspan += 1;
        }
        let filtered = state.search.is_some() || !state.filters.is_empty();
        // Clear only the dimension the link names and keep the rest of the
        // state (follow-up): the URL is rebuilt from the full state, so
        // `group_by` survives, and a "Clear search" link leaves the filters
        // alone.
        let clear_url = filtered.then(|| {
            if state.search.is_some() {
                state.without_search(path)
            } else {
                state.without_filters(path)
            }
        });
        // Search matches anywhere in the value, so the empty copy
        // says "matches", not "prefix matches".
        let message = match &state.search {
            Some(term) => format!("No matches for \u{201c}{term}\u{201d}"),
            None if !state.filters.is_empty() => "No results for these filters".to_string(),
            None => "No records yet".to_string(),
        };
        let clear_label = if state.search.is_some() {
            "Clear search"
        } else {
            "Clear filters"
        };
        // Void window: a cursor that lands past the last row (e.g.
        // rows deleted under pagination) leaves an empty page with no pager —
        // link back to the first page instead of a dead end. State is
        // preserved, only the cursor is dropped.
        let first_page_url =
            (state.after.is_some() || state.before.is_some()).then(|| state.without_cursor(path));
        // Live links write the signals in place (keeping the state the link
        // does not name); `href` stays the no-JS fallback.
        let clear_link: Option<BoxView<'a>> = clear_url.map(|url| {
            let attrs = match signals {
                Some(signals) => {
                    let none = crate::resource::cursor_none();
                    let (q, filters, cursor) = (
                        signals.q.clone(),
                        signals.filters.clone(),
                        signals.cursor.clone(),
                    );
                    let clearing_search = state.search.is_some();
                    attributes! {
                        cx =>
                        href=(url)
                        @click=$(|e: Event| {
                            e.prevent_default();
                            if clearing_search {
                                q.set("".to_owned());
                            } else {
                                filters.set("".to_owned());
                            }
                            cursor.set(none.clone());
                        })
                    }
                }
                None => attributes! { cx => href=(url) },
            };
            view! {
                cx =>
                <a class="text-sm text-primary hover:underline" (attrs)>
                    (clear_label)
                </a>
            }
            .boxed()
        });
        let first_page_link: Option<BoxView<'a>> = first_page_url.map(|url| {
            let attrs = match signals {
                Some(signals) => {
                    let cursor = signals.cursor.clone();
                    let none = crate::resource::cursor_none();
                    attributes! {
                        cx =>
                        href=(url)
                        @click=$(|e: Event| {
                            e.prevent_default();
                            cursor.set(none.clone());
                        })
                    }
                }
                None => attributes! { cx => href=(url) },
            };
            view! {
                cx =>
                <a class="text-sm text-primary hover:underline" (attrs)>
                    "Back to first page"
                </a>
            }
            .boxed()
        });
        Ok(view! {
            cx =>
            table_body(
                table_row(
                    table_cell(
                        attrs: attributes! { colspan=(colspan) class="px-6 py-16 text-center" },
                        <div class="flex flex-col items-center gap-4">
                            <p class="text-sm text-muted-foreground">(message)</p>
                            if let Some(link) = clear_link {
                                (link)
                            }
                            if let Some(link) = first_page_link {
                                (link)
                            }
                        </div>
                    )
                )
            )
        }
        .boxed())
    }

    /// Whether the table renders a row-actions column.
    pub(super) fn with_actions(&self) -> bool {
        self.delete_prefix.is_some() || self.edit_prefix.is_some() || self.view_prefix.is_some()
    }

    /// How many row links sit side by side in the actions column.
    fn action_link_count(&self) -> usize {
        usize::from(self.view_prefix.is_some())
            + usize::from(self.edit_prefix.is_some())
            + usize::from(self.delete_prefix.is_some())
    }

    /// The share of the table the row-actions column claims: the row
    /// links sit side by side and each is a fixed-size control, so the share
    /// grows with the number of links the table renders. The values hold the
    /// widest set at a 1280px window and the narrower sets inside it.
    fn actions_percent(&self) -> u8 {
        match self.action_link_count() {
            2 => 18,
            3.. => 25,
            _ => 12,
        }
    }

    /// The content floor of the row-actions column, in whole rem: one row of
    /// `Md` buttons plus the cell's `p-3` padding, by link count. The share
    /// above is a fraction of the table and shrinks with it, so on a narrow
    /// viewport the buttons would spill past the table and clip against the
    /// chrome's `overflow-hidden`; the floor keeps the column as wide as its
    /// buttons, and the table's `min-width` keeps the table as wide as its
    /// columns, so the wrapper scrolls instead.
    fn actions_min_rem(&self) -> u8 {
        match self.action_link_count() {
            2 => 11,
            3.. => 15,
            _ => 7,
        }
    }

    /// The width every column of this table declares.
    ///
    /// The kind defaults — a [`ColumnWidth::Narrow`] column, the bulk
    /// checkbox, the row actions — are shares of the table, scaled down
    /// together when their nominal total exceeds
    /// [`DEFAULT_WIDTH_BUDGET_PERCENT`]: the wide columns take what the
    /// declared ones leave, and a table that spends every percent on declared
    /// columns leaves them none. An explicit `Rem`/`Percent` is emitted as
    /// declared.
    ///
    /// The table-level `min-width` is the sum of those declarations: every
    /// share as emitted, every `Rem` verbatim, the actions column's content
    /// floor, and one [`WIDE_COLUMN_MIN_REM`] per wide column (which declares
    /// nothing and would otherwise crush to zero). With `w-full` the table
    /// never exceeds its container on its own, so without the floor the
    /// wrapper's `overflow-x-auto` never scrolls; with it the table keeps its
    /// measure on a narrow viewport and the wrapper scrolls. Emitted only
    /// when the sum carries a length — shares alone are a fraction of the
    /// container and can never overflow it.
    pub(super) fn column_widths(&self) -> ColumnWidths
    where
        M: toasty::schema::Model,
    {
        let bulk = self.bulk_enabled().then_some(BULK_COLUMN_PERCENT);
        let actions = self.with_actions().then(|| self.actions_percent());
        let actions_floor = self.with_actions().then(|| self.actions_min_rem());
        let total: u32 = bulk
            .into_iter()
            .chain(
                self.columns
                    .iter()
                    .filter_map(|col| col.column_width().default_percent()),
            )
            .chain(actions)
            .map(u32::from)
            .sum();
        let default_style =
            |nominal: u8| default_width_style(scaled_default_percent(nominal, total));

        let cells = self
            .columns
            .iter()
            .map(|col| {
                let width = col.column_width();
                // `Wide` declares nothing; a kind default is resolved against
                // the rest of the table; `Rem`/`Percent` are verbatim.
                width
                    .explicit_css()
                    .or_else(|| width.default_percent().map(default_style))
            })
            .collect();
        // The `min-width` terms, in layout order: the shares first, then the
        // lengths as one rem total. A scaled share is the emitted one, so the
        // floor and the column agree.
        let mut percent_terms: Vec<u8> = Vec::new();
        if let Some(share) = bulk {
            percent_terms.push(scaled_default_percent(share, total));
        }
        let mut rem_total: u32 = 0;
        for col in &self.columns {
            match col.column_width() {
                ColumnWidth::Wide => rem_total += u32::from(WIDE_COLUMN_MIN_REM),
                ColumnWidth::Narrow => {
                    percent_terms.push(scaled_default_percent(NARROW_DEFAULT_PERCENT, total));
                }
                ColumnWidth::Rem(rem) => rem_total += u32::from(rem),
                ColumnWidth::Percent(share) => percent_terms.push(share),
            }
        }
        let mut actions_style = None;
        if let (Some(share), Some(floor)) = (actions, actions_floor) {
            let scaled = scaled_default_percent(share, total);
            percent_terms.push(scaled);
            rem_total += u32::from(floor);
            actions_style = Some(Cow::Owned(format!(
                "width: {scaled}%; min-width: {floor}rem"
            )));
        }
        let table_min_width = (rem_total > 0).then(|| {
            let mut parts: Vec<String> = percent_terms
                .iter()
                .map(|share| format!("{share}%"))
                .collect();
            parts.push(format!("{rem_total}rem"));
            if parts.len() == 1 {
                Cow::Owned(format!("min-width: {}", parts[0]))
            } else {
                Cow::Owned(format!("min-width: calc({})", parts.join(" + ")))
            }
        });
        ColumnWidths {
            cells,
            bulk: bulk.map(default_style),
            actions: actions_style,
            actions_min: actions_floor.map(|floor| Cow::Owned(format!("min-width: {floor}rem"))),
            table_min_width,
        }
    }

    /// The shared column-header row — the single source of the `<thead>`
    /// markup: labels and **links** on sortable columns that toggle
    /// `?sort=`/`?dir=` (a Lucide arrow with `aria-sort` when active,
    /// `arrow-up-down` when inactive). Every render branch (skeleton / empty
    /// / rows) composes it, so an a11y or styling change happens once.
    ///
    /// With `signals` (a live table) the link also writes the sort signals and
    /// clears the cursors; its `href` stays the no-JS fallback.
    pub(super) async fn render_thead<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        with_actions: bool,
        with_bulk: bool,
        signals: Option<&TableSignals>,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model,
    {
        // The active sort only counts when it names a declared sortable column.
        let active = state.sort.as_ref().filter(|s| {
            self.columns
                .iter()
                .any(|c| c.is_sortable() && c.name() == s.column)
        });
        let widths = self.column_widths();
        let mut heads: Vec<BoxView<'_>> = Vec::with_capacity(self.columns.len());
        for (index, col) in self.columns.iter().enumerate() {
            // Owned per iteration: the view must not borrow the resolved list.
            let width = widths.cells[index].clone();
            let label = col.label().to_string();
            // The declared width rides the header cell's inline `style`
            // a Tailwind class assembled at render would emit no
            // CSS, because Tailwind only generates the literals it finds in
            // source. A wide column declares nothing and takes a share of what
            // the declared columns leave.
            // A static preview renders plain labels: no link to an interaction
            // the page does not honor.
            let sortable = col.is_sortable();
            let (head_class, aria_sort, header) = if sortable {
                let (aria, sort_icon, next_desc) = match active {
                    Some(s) if s.column == col.name() => (
                        if s.descending {
                            "descending"
                        } else {
                            "ascending"
                        },
                        if s.descending {
                            icons::ARROW_DOWN
                        } else {
                            icons::ARROW_UP
                        },
                        // toggling the active column flips the direction
                        !s.descending,
                    ),
                    _ => ("none", icons::ARROW_UP_DOWN, false),
                };
                let href = state.sorted_by(path, col.name(), next_desc);
                let aria_label = format!(
                    "Sort by {} {}",
                    label,
                    if next_desc { "descending" } else { "ascending" }
                );
                let link_attrs = if let Some(signals) = signals {
                    let none = crate::resource::cursor_none();
                    let (sort, dir, cursor) = (
                        signals.sort.clone(),
                        signals.dir.clone(),
                        signals.cursor.clone(),
                    );
                    let column = col.name().to_string();
                    let next_dir = if next_desc { "desc" } else { "asc" }.to_owned();
                    attributes! {
                        cx =>
                        href=(href)
                        aria-label=(aria_label)
                        @click=$(|e: Event| {
                            e.prevent_default();
                            sort.set(column.clone());
                            dir.set(next_dir.clone());
                            cursor.set(none.clone());
                        })
                    }
                } else {
                    attributes! { cx => href=(href) aria-label=(aria_label) }
                };
                (
                    "cursor-pointer hover:bg-foreground/5",
                    Some(aria),
                    view! {
                        cx =>
                        <a
                            class="inline-flex items-center gap-1 hover:text-foreground"
                            (link_attrs)
                        >
                            (label.clone())
                            icon(
                                data: sort_icon,
                                attrs: attributes! { class="size-4 shrink-0 text-muted-foreground" }
                            )
                        </a>
                    }
                    .boxed(),
                )
            } else {
                ("", None, view! { cx => (label.clone()) }.boxed())
            };
            heads.push(
                view! {
                    cx =>
                    table_head(
                        attrs: attributes! {
                            class=(head_class)
                            aria-sort=(aria_sort)
                            style=(width.as_deref())
                        },
                        (header)
                    )
                }
                .boxed(),
            );
        }
        if with_actions {
            heads.push(
                view! {
                    cx =>
                    // The share sizes the column at wide viewports; the floor
                    // sizes it to its buttons at narrow ones, where the share
                    // alone would let them spill past the table. Each row's
                    // `td` repeats only the floor.
                    table_head(
                        attrs: attributes! { style=(widths.actions.as_deref()) },
                        "Actions"
                    )
                }
                .boxed(),
            );
        }
        Ok(view! {
            cx =>
            table_header(
                table_row(
                    if with_bulk {
                        // The header row is the row `table-fixed` measures, so
                        // the chrome columns declare their width here and
                        // their `td`s declare none.
                        table_head(
                            attrs: attributes! { style=(widths.bulk.as_deref()) },
                            <input
                                type="checkbox"
                                aria-label="Select all rows"
                                data-bulk-select-all=""
                            >
                        )
                    }
                    for h in heads {
                        (h)
                    }
                )
            )
        }
        .boxed())
    }
}

/// Precomputed per-row presentation for the table body: the display row key,
/// the record key, the rendered cells, and the optional Edit / delete-dialog
/// action URLs. A struct (not a tuple): five anonymous positions would
/// mislead readers and trip `clippy::type_complexity`.
///
/// `key` is the display projection (keyed diffs, DOM ids);
/// `record_id` is the record projection (URLs, bulk values), resolved
/// by handlers as the typed PK.
struct RowView {
    key: String,
    record_id: String,
    cells: Vec<String>,
    view_url: Option<String>,
    edit_url: Option<String>,
    delete_url: Option<String>,
    /// The row's delete POST target (`{prefix}/{key}/delete`): the
    /// Delete control hands it to the shared dialog before opening it, so the
    /// confirmed POST keeps the route the `?delete=` fallback uses.
    delete_action: Option<String>,
    /// Whether the row renders a bulk checkbox: a row the
    /// [`Table::row_actions`] policy denies `delete` renders none, so
    /// `bulk.js` never sees its key and select-all cannot submit a batch the
    /// handler refuses wholesale.
    selectable: bool,
    /// The row's group label, when `?group_by=` named the declared group.
    /// Carried on every row so the page-local shim can order by it.
    group: Option<String>,
    /// The header this row renders above itself, `Some` only on the first row
    /// of its group.
    group_header: Option<GroupHeader>,
}

/// One page-local group header: the label with its page-local count,
/// and the stable DOM id the injected header row carries so the in-place morph
/// can follow it (`row_dom_id`'s contract).
#[derive(Clone)]
struct GroupHeader {
    /// `"{label} ({n} on this page)"`.
    text: String,
    /// [`group_header_dom_id`] of the label.
    dom_id: String,
}
#[cfg(test)]
pub(super) mod tests {
    use topcoat::context::CxTestBuilder;

    use super::*;
    use crate::resource::{ColumnWidth, SelectFilter, Sort, TextColumn, VariantFilter};

    #[derive(Debug, Clone, toasty::Model)]
    pub(crate) struct User {
        #[key]
        #[auto]
        pub(crate) id: uuid::Uuid,
        pub(crate) name: String,
    }

    #[derive(Debug, Clone, toasty::Model)]
    pub(crate) struct Task {
        #[key]
        #[auto]
        pub(crate) id: uuid::Uuid,
        pub(crate) title: String,
        pub(crate) status: String,
        pub(crate) featured: bool,
        pub(crate) created_at: jiff::Timestamp,
    }

    #[derive(Debug, Clone, PartialEq, toasty::Embed)]
    pub(crate) enum Vehicule {
        Auto {
            #[shared(puissance)]
            puissance: String,
            seats: String,
        },
        Moto {
            #[shared(puissance)]
            puissance: String,
            cc: String,
        },
    }

    #[derive(Debug, Clone, toasty::Model)]
    pub(crate) struct Driver {
        #[key]
        #[auto]
        pub(crate) id: uuid::Uuid,
        pub(crate) name: String,
        vehicule: Vehicule,
    }

    pub(crate) fn vehicule_filter() -> VariantFilter<Driver> {
        VariantFilter::r#for(
            "vehicule",
            "Véhicule",
            vec![
                ("Auto".to_string(), Driver::fields().vehicule().is_auto()),
                ("Moto".to_string(), Driver::fields().vehicule().is_moto()),
            ],
        )
    }

    pub(crate) fn status_table(_cx: &Cx) -> Table<Task> {
        Table::<Task>::new(
            |t| t.id.to_string(),
            TextColumn::r#for(Task::fields().title(), |t| t.title.clone()),
        )
        .filters(SelectFilter::r#for(
            Task::fields().status(),
            vec!["published".to_string(), "draft".to_string()],
        ))
    }

    pub(crate) fn filters_state(pairs: &[(&str, &str)]) -> TableState {
        TableState {
            filters: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            ..TableState::default()
        }
    }

    pub(crate) fn last_link_named<'a>(html: &'a str, label: &str) -> &'a str {
        html.rsplit('<')
            .find(|chunk| chunk.contains(label))
            .unwrap_or_else(|| panic!("missing {label} link in {html}"))
    }

    /// The `<table …>` opening tag of a rendered table, without its children.
    pub(crate) fn table_tag(html: &str) -> &str {
        let start = html.find("<table").expect("the table element");
        let end = html[start..].find('>').expect("its tag end") + start;
        &html[start..end]
    }

    /// The layout a `<table ...>` tag declares, independent of attribute
    /// order: the sorted `class`/`style` values the tag carries. Two tags
    /// declaring the same layout compare equal even when the serializer
    /// emits `style` before `class` in one and after it in the other.
    pub(crate) fn normalized_table_tag(tag: &str) -> (String, String) {
        (table_attr(tag, "class"), table_attr(tag, "style"))
    }

    /// The value of one quoted attribute inside a tag, or empty when absent.
    pub(crate) fn table_attr(tag: &str, name: &str) -> String {
        let marker = format!("{name}=\"");
        let Some(at) = tag.find(&marker) else {
            return String::new();
        };
        let rest = &tag[at + marker.len()..];
        let mut classes: Vec<&str> = rest
            .split('"')
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .collect();
        classes.sort_unstable();
        classes.join(" ")
    }

    /// Every whole-percent width a rendered table declares, in document order.
    /// A length declaration is skipped: those carry a unit. A share paired
    /// with a content floor (`width: 18%; min-width: 11rem`) still parses: the
    /// share ends at the `;`, not at the attribute's closing quote.
    pub(crate) fn declared_percents(html: &str) -> Vec<u32> {
        html.match_indices("style=\"width: ")
            .filter_map(|(at, marker)| {
                html[at + marker.len()..]
                    .split(['"', ';'])
                    .next()?
                    .strip_suffix('%')?
                    .parse()
                    .ok()
            })
            .collect()
    }

    #[tokio::test]
    async fn table_for_columns_renders_with_keyed_rows() {
        let cx = CxTestBuilder::new().build();
        // columns need distinct names — title + status, not one
        // field twice.
        let tasks_table = Table::<Task>::new(
            |t| t.id.to_string(),
            (
                TextColumn::r#for(Task::fields().title(), |t: &Task| t.title.clone()).searchable(),
                TextColumn::r#for(Task::fields().status(), |t: &Task| t.status.clone()).sortable(),
            ),
        );
        // Use dummy rows for render check (no DB) — keyed by row.id
        let rows = vec![
            Task {
                id: uuid::Uuid::new_v4(),
                title: "Ada".to_string(),
                status: "draft".to_string(),
                featured: false,
                created_at: jiff::Timestamp::now(),
            },
            Task {
                id: uuid::Uuid::new_v4(),
                title: "Bob".to_string(),
                status: "published".to_string(),
                featured: true,
                created_at: jiff::Timestamp::now(),
            },
        ];
        let page: TablePage<Task> = rows.clone().into();
        let html = tasks_table
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        // no Tailwind-class assertions. The chrome literals
        // (`rounded-xl`, `border-border`, `text-muted-foreground`,
        // `cursor-pointer`) are the showcase's business (#136), and pinning
        // them here meant every restyle broke a core test.
        //
        // Searchable columns render no extra header chrome: the
        // search input is the affordance, so the header cell holds its label
        // and nothing interactive. The sortable sibling next door *does* carry
        // an `<a>` and an icon, so this can fail.
        let title_at = html.find("Title").expect("the Title header");
        let title_th = html[..title_at].rfind("<th").expect("its <th>");
        let title_th_end = html[title_th..].find("</th>").expect("its </th>") + title_th;
        let title_head = &html[title_th..title_th_end];
        assert!(
            !title_head.contains("<svg") && !title_head.contains("<a "),
            "a searchable header must render no sort or loupe chrome, got {title_head}"
        );
        // Sortable ones carry the inactive `arrow-up-down` with
        // `aria-sort="none"`.
        assert!(
            html.contains("aria-sort=\"none\""),
            "missing sortable indicator in {html}"
        );
        assert!(html.contains("Title"), "missing Title header in {html}");
        assert!(html.contains("Status"), "missing Status header in {html}");
        for row in &rows {
            assert!(
                html.contains(&row.title),
                "missing row title {} in {html}",
                row.title
            );
        }
    }

    /// the table lays out fixed, and a declared column width reaches
    /// the header cell and every row's cell as data — an inline `style`, never
    /// a Tailwind class built at render.
    #[tokio::test]
    async fn table_lays_out_fixed_and_emits_declared_column_widths() {
        let cx = CxTestBuilder::new().build();
        let width_table = Table::<Task>::new(
            |t| t.id.to_string(),
            (
                // A field-backed column defaults to `Wide`: it declares no
                // width and takes the share the declared columns leave.
                TextColumn::r#for(Task::fields().title(), |t: &Task| t.title.clone()),
                // A computed column defaults to `Narrow`, overridden here.
                TextColumn::computed("Status", |t: &Task| t.status.clone())
                    .width(ColumnWidth::Percent(30)),
            ),
        );
        let page: TablePage<Task> = vec![Task {
            id: uuid::Uuid::new_v4(),
            title: "Ada".to_string(),
            status: "draft".to_string(),
            featured: false,
            created_at: jiff::Timestamp::now(),
        }]
        .into();
        let html = width_table
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        // The fixed layout is the table's own contract, not paint: the
        // Done-when names it as the observable and a class is its only
        // transport, so this is the one class literal asserted here. The paint
        // classes stay the showcase's business (#136).
        let tag = table_tag(&html);
        assert!(
            tag.contains("table-fixed"),
            "the table must lay out fixed, got {tag}"
        );
        // The declared width is data on the header and on the row's cell: one
        // declaration, two carriers.
        assert_eq!(
            html.matches("style=\"width: 30%\"").count(),
            2,
            "the declared width must reach the th and the td, got {html}"
        );
        // The wide column declares nothing: an absent attribute, not a
        // generated class.
        assert_eq!(
            html.matches("style=\"width").count(),
            2,
            "only the declared column carries a width, got {html}"
        );
    }

    /// a column that declares nothing but its kind claims a share of
    /// the table — a percentage, so it shrinks with the table instead of
    /// outgrowing it — and the wide column beside it still declares none.
    #[tokio::test]
    async fn kind_defaults_claim_a_share_of_the_table() {
        let cx = CxTestBuilder::new().build();
        let default_table = Table::<Task>::new(
            |t| t.id.to_string(),
            (
                TextColumn::r#for(Task::fields().title(), |t: &Task| t.title.clone()),
                TextColumn::computed("Status", |t: &Task| t.status.clone()),
            ),
        );
        let page: TablePage<Task> = vec![Task {
            id: uuid::Uuid::new_v4(),
            title: "Ada".to_string(),
            status: "draft".to_string(),
            featured: false,
            created_at: jiff::Timestamp::now(),
        }]
        .into();
        let html = default_table
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert_eq!(
            declared_percents(&html),
            [10, 10],
            "a computed column claims its kind's share on the th and the td, got {html}"
        );
        assert_eq!(
            html.matches("style=\"width").count(),
            2,
            "the field column must declare nothing, got {html}"
        );
    }

    /// the chrome columns declare a share of the table too — the
    /// header row is the row `table-fixed` measures — and the share grows with
    /// the number of row links, which sit side by side. The actions column
    /// pairs its share with a content floor (`min-width: 11rem`), on the
    /// header and on every row's cell, so the buttons fit instead of spilling
    /// past the table on a narrow viewport.
    #[tokio::test]
    async fn chrome_columns_declare_their_widths() {
        // Each case: the row links to wire, the share Actions claims, and the
        // floor that holds its buttons.
        let cases: [(usize, &str, &str); 3] =
            [(1, "12%", "7rem"), (2, "18%", "11rem"), (3, "25%", "15rem")];
        for (links, expected, floor) in cases {
            let cx = CxTestBuilder::new().build();
            let mut chrome_table = Table::<User>::new(
                |u| u.id.to_string(),
                TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
            )
            .with_view("/admin/users".to_string());
            if links > 1 {
                chrome_table = chrome_table.with_edit("/admin/users".to_string());
            }
            if links > 2 {
                // Delete is what the bulk column pairs with.
                chrome_table = chrome_table
                    .with_delete("/admin/users".to_string())
                    .with_bulk_delete(true);
            }
            let page: TablePage<User> = vec![User {
                id: uuid::Uuid::new_v4(),
                name: "Ada".to_string(),
            }]
            .into();
            let html = chrome_table
                .render(&cx, page)
                .await
                .unwrap()
                .single()
                .await
                .unwrap()
                .render(&cx);
            assert_eq!(
                html.matches(&format!("width: {expected}")).count(),
                1,
                "{links} row links must claim {expected} in the header row, got {html}"
            );
            // The floor rides the header and every row's cell, so the buttons
            // fit whatever the share shrinks to.
            assert_eq!(
                html.matches(&format!("min-width: {floor}")).count(),
                2,
                "{links} row links must floor the actions column at {floor}, got {html}"
            );
            // The bulk checkbox claims its own share, and only when the table
            // renders one.
            let bulk = if links > 2 { 1 } else { 0 };
            assert_eq!(
                html.matches(&format!("style=\"width: {BULK_COLUMN_PERCENT}%\""))
                    .count(),
                bulk,
                "the bulk column's share must follow the table's chrome, got {html}"
            );
        }
    }

    /// the kind defaults together stay inside their budget, whatever
    /// the column set — a column that declares none is rendered at zero width
    /// once the declared shares claim the whole table, header text included,
    /// so the defaults scale down instead of spending the last percent.
    #[tokio::test]
    async fn kind_defaults_stay_inside_their_budget() {
        let cx = CxTestBuilder::new().build();
        // Four computed columns (4 × the 10% nominal) plus both chrome columns
        // (5% + 20%) overrun the budget, so every default is scaled down
        // together and the field column beside them keeps the rest.
        let crowded = Table::<Task>::new(
            |t| t.id.to_string(),
            (
                TextColumn::r#for(Task::fields().title(), |t: &Task| t.title.clone()),
                TextColumn::computed("Status", |t: &Task| t.status.clone()),
                TextColumn::computed("Featured", |t: &Task| t.featured.to_string()),
                TextColumn::computed("Created", |t: &Task| t.created_at.to_string()),
                TextColumn::computed("Id", |t: &Task| t.id.to_string()),
            ),
        )
        .with_delete("/admin/tasks".to_string())
        .with_edit("/admin/tasks".to_string())
        .with_view("/admin/tasks".to_string())
        .with_bulk_delete(true);
        let page: TablePage<Task> = vec![Task {
            id: uuid::Uuid::new_v4(),
            title: "Ada".to_string(),
            status: "draft".to_string(),
            featured: false,
            created_at: jiff::Timestamp::now(),
        }]
        .into();
        let html = crowded
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        // One share per declared column, in the header row: the four computed
        // columns and the two chrome columns.
        let thead_at = html.find("<thead").expect("a header row");
        let thead_end = html.find("</thead>").expect("its end");
        let percents = declared_percents(&html[thead_at..thead_end]);
        assert_eq!(
            percents.len(),
            6,
            "one share per declared column, got {percents:?} in {html}"
        );
        assert!(
            percents.iter().all(|percent| *percent > 0),
            "a scaled share must keep its column visible, got {percents:?}"
        );
        let total: u32 = percents.iter().sum();
        assert!(
            total <= u32::from(DEFAULT_WIDTH_BUDGET_PERCENT),
            "the kind defaults must leave the field column a share, got {percents:?}"
        );
        // The field column declares nothing at all, so it takes what the
        // declared columns leave.
        let title_at = html.find(">Title<").expect("the Title header");
        let title_th = html[..title_at].rfind("<th").expect("its <th>");
        assert!(
            !html[title_th..title_at].contains("style="),
            "the field column must declare no width, got {}",
            &html[title_th..title_at]
        );
        // Every share rides its header cell, and each text column repeats its
        // own on the row's cell: four text columns twice, two chrome once.
        assert_eq!(
            declared_percents(&html).len(),
            10,
            "each share must reach its th and its td, got {html}"
        );
    }

    #[tokio::test]
    async fn edit_links_render_beside_delete_in_actions_column() {
        // GH #162 (Filament's `recordActions` EditAction): `with_edit` wires
        // one `Edit` link per row into the shared Actions column.
        let cx = CxTestBuilder::new().build();
        let action_table = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        )
        .with_delete("/admin/users".to_string())
        .with_edit("/admin/users".to_string());
        let rows = vec![User {
            id: uuid::Uuid::new_v4(),
            name: "Ada".to_string(),
        }];
        let id = rows[0].id.to_string();
        let page: TablePage<User> = rows.into();
        let html = action_table
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(html.contains("Actions"), "missing Actions header in {html}");
        assert!(
            html.contains(&format!("href=\"/admin/users/{id}/edit\"")) && html.contains(">Edit<"),
            "missing Edit link for {id} in {html}"
        );
        assert!(
            html.contains("Delete"),
            "Delete link must survive, got {html}"
        );
        // Without either prefix there is no Actions column at all — and a
        // chromeless table needs no `pk`: nothing emits URLs.
        let plain = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        );
        let page: TablePage<User> = vec![User {
            id: uuid::Uuid::new_v4(),
            name: "Ada".to_string(),
        }]
        .into();
        let html = plain
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            !html.contains("Actions") && !html.contains(">Edit<"),
            "plain table must not render action chrome, got {html}"
        );
    }

    #[tokio::test]
    async fn denied_rows_render_no_links_and_no_checkbox() {
        // the row policy gates the chrome per record, so a row the
        // resource refuses renders no Edit/Delete link and no bulk checkbox —
        // the rendered affordance and the route agree.
        let cx = CxTestBuilder::new().build();
        let ada = User {
            id: uuid::Uuid::new_v4(),
            name: "Ada".to_string(),
        };
        let ken = User {
            id: uuid::Uuid::new_v4(),
            name: "Ken".to_string(),
        };
        let ken_id = ken.id.to_string();
        let ada_id = ada.id.to_string();
        let policy_table = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        )
        .with_delete("/admin/users".to_string())
        .with_edit("/admin/users".to_string())
        .with_view("/admin/users".to_string())
        .with_bulk_delete(true)
        .row_actions(|u: &User| RowActions {
            view: true,
            edit: u.name != "Ken",
            delete: u.name != "Ken",
        });
        let page: TablePage<User> = vec![ada, ken].into();
        let html = policy_table
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        // The allowed row keeps all three links and an enabled checkbox.
        assert!(
            html.contains(&format!("href=\"/admin/users/{ada_id}/edit\""))
                && html.contains(&format!("href=\"/admin/users/{ada_id}\""))
                && html.contains(&format!("href=\"?delete={ada_id}\"")),
            "the allowed row must keep its View/Edit/Delete links, got {html}"
        );
        // The denied row keeps only the View link its policy allows: no Edit
        // link and no delete dialog opener.
        assert!(
            html.contains(&format!("href=\"/admin/users/{ken_id}\""))
                && !html.contains(&format!("/admin/users/{ken_id}/edit"))
                && !html.contains(&format!("delete={ken_id}")),
            "the denied row must render no Edit/Delete link, got {html}"
        );
        // Its row still renders, but with no checkbox at all: the one
        // `data-row-select` on the page is the allowed row's.
        assert!(
            html.contains(">Ken<"),
            "the denied row must still render, got {html}"
        );
        assert!(
            !html.contains(&format!("value=\"{ken_id}\"")),
            "the denied row must render no checkbox, got {html}"
        );
        assert_eq!(
            html.matches("data-row-select").count(),
            1,
            "the allowed row owns the page's only checkbox, got {html}"
        );
        // The allowed row's checkbox is present, so the count above is not
        // passing on a page with no bulk chrome at all.
        assert!(
            html.contains(&format!("value=\"{ada_id}\"")),
            "the allowed row must keep its checkbox, got {html}"
        );
    }

    /// The `<tr>…</tr>` chunk holding the row checkbox with `value`, without
    /// its closing tag: the row's own cells scoped down from the page.
    pub(crate) fn row_chunk<'a>(html: &'a str, value: &str) -> &'a str {
        let at = html
            .find(&format!("value=\"{value}\""))
            .unwrap_or_else(|| panic!("missing the row checkbox with value {value}"));
        let start = html[..at].rfind("<tr").expect("the row's opening tag");
        let end = html[at..].find("</tr>").expect("the row's closing tag") + at;
        &html[start..end]
    }

    #[tokio::test]
    async fn fully_locked_rows_keep_their_actions_cell_with_no_links() {
        // A row the policy locks out of every link keeps its actions cell all
        // the same: the cell stays aligned with the header instead of going
        // missing, and the row carries as many cells as the header.
        let cx = CxTestBuilder::new().build();
        let ada = User {
            id: uuid::Uuid::new_v4(),
            name: "Ada".to_string(),
        };
        let ken = User {
            id: uuid::Uuid::new_v4(),
            name: "Ken".to_string(),
        };
        let ada_id = ada.id.to_string();
        let policy_table = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        )
        .with_delete("/admin/users".to_string())
        .with_edit("/admin/users".to_string())
        .with_bulk_delete(true)
        .row_actions(|u: &User| {
            let allowed = u.name != "Ken";
            RowActions {
                view: allowed,
                edit: allowed,
                delete: allowed,
            }
        });
        let page: TablePage<User> = vec![ada, ken].into();
        let html = policy_table
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        // The locked row renders no action link at all. Found by its name
        // cell: it carries no checkbox value to search for.
        let ken_at = html.find(">Ken<").expect("the locked row");
        let ken_start = html[..ken_at].rfind("<tr").expect("its row");
        let ken_end = html[ken_at..].find("</tr>").expect("its end") + ken_at;
        let ken_row = &html[ken_start..ken_end];
        assert!(
            !ken_row.contains("/admin/users/"),
            "the locked row must render no action link at all, got {ken_row}"
        );
        // The allowed row keeps its links, so the absence above is not
        // passing on a page that renders no chrome at all.
        let ada_row = row_chunk(&html, &ada_id);
        assert!(
            ada_row.contains(&format!("/admin/users/{ada_id}/edit")),
            "the allowed row must keep its links, got {ada_row}"
        );
        // Alignment: the locked row carries a cell per header. Counted on
        // the closing tags: `<thead` itself opens with `<th`.
        let thead_at = html.find("<thead").expect("a header row");
        let thead_end = html.find("</thead>").expect("its end");
        assert_eq!(
            ken_row.matches("</td>").count(),
            html[thead_at..thead_end].matches("</th>").count(),
            "the locked row must carry a cell per header, got {ken_row}"
        );
    }

    #[tokio::test]
    async fn a_chromeless_table_never_consults_the_row_policy() {
        // the policy is consulted only where chrome is wired, so a
        // resource that declares no chrome keeps its list page free of
        // per-record predicate calls — the coarse `TableChrome` gate is intact.
        let cx = CxTestBuilder::new().build();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = calls.clone();
        let policy_table = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        )
        .row_actions(move |_: &User| {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            RowActions::ALL
        });
        let page: TablePage<User> = vec![User {
            id: uuid::Uuid::new_v4(),
            name: "Ada".to_string(),
        }]
        .into();
        let html = policy_table
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains("Ada"),
            "the table must still render its row, got {html}"
        );
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "a table with no action prefix must not call the row policy"
        );
    }

    #[tokio::test]
    async fn action_chrome_emits_record_keys_not_display_keys() {
        // a non-PK display projection drives keyed diffs and DOM ids
        // only — edit URLs, delete dialogs, and bulk values carry the record
        // projection handlers resolve as the typed PK.
        use topcoat::view::ViewExt;
        let cx = CxTestBuilder::new().build();
        let key_table = Table::<User>::new_split(
            |u| u.id.to_string().to_uppercase(),
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        )
        .with_delete("/admin/users".to_string())
        .with_edit("/admin/users".to_string())
        .with_bulk_delete(true);
        let rows = vec![User {
            id: uuid::Uuid::new_v4(),
            name: "Ada".to_string(),
        }];
        let lower = rows[0].id.to_string();
        let upper = lower.to_uppercase();
        let page: TablePage<User> = rows.into();
        let html = key_table
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        // URLs and bulk values: canonical PK text.
        assert!(
            html.contains(&format!("href=\"/admin/users/{lower}/edit\"")),
            "edit URL must carry the record key in {html}"
        );
        assert!(
            html.contains(&format!("value=\"{lower}\"")),
            "bulk value must carry the record key in {html}"
        );
        assert!(
            html.contains(&format!("?delete={lower}")),
            "delete dialog link must carry the record key in {html}"
        );
        assert!(
            !html.contains(&format!("value=\"{upper}\"")),
            "display key must never be a bulk value in {html}"
        );
        // Display key still drives the DOM identity.
        assert!(
            html.contains(&upper),
            "display key must still render (DOM/keyed diff) in {html}"
        );
    }

    #[tokio::test]
    async fn pk_only_table_renders_display_from_record_key() {
        // a single-key table drives keyed diffs, DOM ids, and chrome URLs
        // from the one projection.
        use topcoat::view::ViewExt;
        let cx = CxTestBuilder::new().build();
        let tbl = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        )
        .with_delete("/admin/users".to_string())
        .with_edit("/admin/users".to_string())
        .with_bulk_delete(true);
        let rows = vec![User {
            id: uuid::Uuid::new_v4(),
            name: "Ada".to_string(),
        }];
        let key = rows[0].id.to_string();
        let page: TablePage<User> = rows.into();
        let html = tbl
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains(&format!("href=\"/admin/users/{key}/edit\"")),
            "edit URL must carry the record key in {html}"
        );
        assert!(
            html.contains(&format!("value=\"{key}\"")),
            "bulk value must carry the record key in {html}"
        );
        assert!(
            html.contains(&format!("?delete={key}")),
            "delete dialog link must carry the record key in {html}"
        );
        assert!(
            html.contains(&format!(
                "id=\"{}\"",
                crate::resource::state::row_dom_id(&key)
            )),
            "the row DOM id must fall back to the record key in {html}"
        );
    }

    #[tokio::test]
    async fn id_only_non_pk_display_emits_display_urls() {
        // `new` declares both halves together, so a non-PK display without
        // `new_split` renders action URLs from the display value — handlers
        // 404 them. Authors must use `new_split` (see the tables guide).
        use topcoat::view::ViewExt;
        let cx = CxTestBuilder::new().build();
        let tbl = Table::<User>::new(
            |u| u.name.clone(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        )
        .with_delete("/admin/users".to_string())
        .with_edit("/admin/users".to_string())
        .with_bulk_delete(true);
        let page: TablePage<User> = vec![User {
            id: uuid::Uuid::new_v4(),
            name: "Ada".to_string(),
        }]
        .into();
        let html = tbl
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains("href=\"/admin/users/Ada/edit\""),
            "edit URL carries the display value without a split record key in {html}"
        );
        assert!(
            html.contains("value=\"Ada\""),
            "bulk value carries the display value without a split record key in {html}"
        );
        assert!(
            html.contains("?delete=Ada"),
            "delete dialog link carries the display value without a split record key in {html}"
        );
    }

    #[tokio::test]
    async fn group_by_unknown_value_renders_no_headers_and_drops_param() {
        // `?group_by=` must name the declared group — any other
        // value renders no headers and vanishes from pager links instead of
        // silently grouping by the single declared key.
        let cx = CxTestBuilder::new().build();
        let grouped = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()).sortable(),
        )
        .group_by("status", |u| u.name.clone())
        .paginate(1);
        let state = TableState {
            group_by: Some("email".to_string()),
            sort: Some(Sort {
                column: "name".to_string(),
                descending: false,
            }),
            ..TableState::default()
        };
        let rows = vec![User {
            id: uuid::Uuid::nil(),
            name: "Ada".to_string(),
        }];
        let page = TablePage {
            rows,
            next_cursor: Some("abc".to_string()),
            prev_cursor: None,
        };
        let html = grouped
            .render_with_state(&cx, page, &state, "/admin/users")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            !html.contains("on this page"),
            "unknown group_by must render no headers, got {html}"
        );
        assert!(
            !html.contains("group_by"),
            "unknown group_by must drop from links, got {html}"
        );
    }

    #[tokio::test]
    async fn group_by_orders_each_row_under_its_own_header() {
        // the page-local shim must actually group. The seed is
        // deliberately interleaved in query order (draft, published, draft,
        // published), so a legend-only shim — every header, then an ungrouped
        // table — cannot satisfy the ordering assertions below.
        let cx = CxTestBuilder::new().build();
        let grouped = Table::<Task>::new(
            |t| t.id.to_string(),
            TextColumn::r#for(Task::fields().title(), |t| t.title.clone()),
        )
        .group_by("status", |t| t.status.clone());
        let state = TableState {
            group_by: Some("status".to_string()),
            ..TableState::default()
        };
        let task = |title: &str, status: &str| Task {
            id: uuid::Uuid::new_v4(),
            title: title.to_string(),
            status: status.to_string(),
            featured: false,
            created_at: jiff::Timestamp::now(),
        };
        let page = TablePage::from(vec![
            task("alpha", "draft"),
            task("bravo", "published"),
            task("charlie", "draft"),
            task("delta", "published"),
        ]);
        let html = grouped
            .render_with_state(&cx, page, &state, "/admin/tasks")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        let at = |needle: &str| {
            html.find(needle)
                .unwrap_or_else(|| panic!("missing {needle:?} in {html}"))
        };
        // Rows are ordered by the group key (draft before published) and each
        // header sits immediately above its own rows — the stable sort keeps
        // the query's order inside a group.
        let draft_header = at("draft (2 on this page)");
        let alpha = at("alpha");
        let charlie = at("charlie");
        let published_header = at("published (2 on this page)");
        let bravo = at("bravo");
        let delta = at("delta");
        assert!(
            draft_header < alpha && alpha < charlie,
            "both draft rows must sit under the draft header, got {html}"
        );
        assert!(
            charlie < published_header,
            "the published header must follow the draft group, got {html}"
        );
        assert!(
            published_header < bravo && bravo < delta,
            "both published rows must sit under the published header, got {html}"
        );
        // The injected header carries an id derived from its group label, not
        // from its position, so the in-place morph can follow it:
        // the same contract the row ids have.
        for label in ["draft", "published"] {
            let expected = format!("id=\"{}\"", group_header_dom_id(label));
            assert!(
                html.contains(&expected),
                "the {label} header needs the stable id {expected:?}, got {html}"
            );
        }
    }

    /// a table render builds the row-action URLs from one shared base
    /// — the encoded filter transport — before the row loop, so every row's
    /// dialog opener is that base plus its own `delete=` key.
    ///
    /// The base cannot be observed as a count: `filters_param` is a pure
    /// function of the state, so a per-row rebuild produces identical bytes.
    /// This pins the shape instead — every opener shares byte-identical bytes
    /// before `delete=`, independent of the page size.
    #[tokio::test]
    async fn table_render_reuses_one_filter_transport_base_across_rows() {
        let cx = CxTestBuilder::new().build();
        let state = filters_state(&[("status", "published"), ("featured", "true")]);
        let tbl = Table::<User>::new(
            |u: &User| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone()),
        )
        .with_delete("/admin/users".to_string());
        let rows = |n: usize| -> Vec<User> {
            (0..n)
                .map(|i| User {
                    id: uuid::Uuid::from_u128(i as u128),
                    name: format!("user-{i}"),
                })
                .collect()
        };
        let render = async |page: TablePage<User>| {
            tbl.render_with_state(&cx, page, &state, "/admin/users")
                .await
                .unwrap()
                .single()
                .await
                .unwrap()
                .render(&cx)
        };
        // Each row's opener, keyed off the one attribute only an action link
        // carries, with the per-row `delete` key stripped: what is left is the
        // page's shared base.
        fn delete_bases(html: &str) -> Vec<&str> {
            html.split("href=\"")
                .skip(1)
                .filter_map(|chunk| chunk.split('"').next())
                .filter(|href| href.contains("delete="))
                .map(|href| href.split("delete=").next().unwrap())
                .collect()
        }

        let one_html = render(TablePage::from(rows(1))).await;
        let eight_html = render(TablePage::from(rows(8))).await;
        let one_row = delete_bases(&one_html);
        let eight_rows = delete_bases(&eight_html);
        assert_eq!(one_row.len(), 1, "one row, one dialog opener");
        assert_eq!(eight_rows.len(), 8, "eight rows, eight dialog openers");
        // The sorted, query-encoded transport every row's link must carry.
        let transport = "filters=featured%3Atrue%2Cstatus%3Apublished";
        for base in one_row.iter().chain(eight_rows.iter()) {
            assert!(
                base.contains(transport),
                "every opener must carry the page's filter transport, got {base}"
            );
            assert_eq!(
                *base, one_row[0],
                "rows must reuse the page's one encoded base, not rebuild it per row"
            );
        }
    }

    #[tokio::test]
    async fn a_static_table_renders_no_runtime_bindings_at_all() {
        // a table without `live_search` has no shard to re-run, so a
        // mutation replaces its region with the response's. That is only sound
        // because the region is inert: no binding, no handler, nothing the
        // replacement could leave dead. The refresh control's absence is the
        // page's own answer to "can this table refresh in place?".
        let cx = CxTestBuilder::new().build();
        let tbl = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        )
        .with_delete("/admin/users".to_string())
        .with_bulk_delete(true);
        let rows = vec![User {
            id: uuid::Uuid::nil(),
            name: "Ada".to_string(),
        }];
        let html = tbl
            .render_with_state(&cx, rows.into(), &TableState::default(), "/admin/users")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            !html.contains("data-table-revision"),
            "a static table must carry no refresh control, got {html}"
        );
        assert!(
            !html.contains("data-topcoat-"),
            "a static table's region must be inert markup, got {html}"
        );
    }

    #[tokio::test]
    async fn rendered_rows_carry_stable_dom_ids() {
        // every rendered row exposes its morph id; re-rendering the
        // same page yields the same ids.
        let cx = CxTestBuilder::new().build();
        let tbl = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        );
        let rows = vec![
            User {
                id: uuid::Uuid::nil(),
                name: "Ada".to_string(),
            },
            User {
                id: uuid::Uuid::max(),
                name: "Alan".to_string(),
            },
        ];
        let render = async |rows: Vec<User>| {
            tbl.render_with_state(&cx, rows.into(), &TableState::default(), "/admin/users")
                .await
                .unwrap()
                .single()
                .await
                .unwrap()
                .render(&cx)
        };
        let first = render(rows.clone()).await;
        assert!(
            first.contains("id=\"row-00000000-0000-0000-0000-000000000000-"),
            "missing morph id for first row, got {first}"
        );
        assert!(
            first.contains("id=\"row-ffffffff-ffff-ffff-ffff-ffffffffffff-"),
            "missing morph id for second row, got {first}"
        );
        let second = render(rows).await;
        assert_eq!(
            first.matches("id=\"row-").count(),
            second.matches("id=\"row-").count(),
            "reruns must keep stable row ids"
        );
    }
}
