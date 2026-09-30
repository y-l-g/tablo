//! Static and live entry points and the table chrome they share.

use tablo_ui::{table, table_body};
use topcoat::{Result, context::Cx, runtime::Event, view::*};

use super::{
    super::{
        super::{
            page::TablePage,
            state::{TableSignals, TableState},
        },
        Table,
    },
    BAR_CLASS, TABLE_BARE_CLASS, TABLE_CARD_CLASS,
    rows::{RowChrome, render_rows},
    widths::ColumnWidths,
};

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
    /// `cx`: a page that owns its table passes the state it parsed and the
    /// list URL its links should target.
    ///
    /// Normalizes the state it is handed, so an unknown `?group_by=` never
    /// echoes through a link.
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
        self.render_inner(cx, page, &self.normalize_state(state), path, None)
            .await
    }

    /// Render the interactive body for a live table: the same
    /// presentation as [`Self::render_with_state`], with the sort links, the
    /// pager, the filter bar, and the empty-state clear links writing the
    /// `query` signal — each interaction writes it and the browser morphs the
    /// shard's new output in place, without a navigation or a scroll jump.
    /// Every bound control keeps its real `href`/form, so a page without JS
    /// still navigates as before.
    ///
    /// The row-delete dialog is not part of this output: it lives
    /// outside the region a rerun swaps, rendered once by the page that owns
    /// the signals, so a caller rendering only through this method renders
    /// [`Self::render_delete_dialog`] itself to keep the `?delete=` fallback.
    pub(crate) async fn render_live<'a>(
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
        self.render_inner(cx, page, state, path, Some(signals))
            .await
    }

    async fn render_inner<'a>(
        &self,
        cx: &'a Cx,
        page: TablePage<M>,
        state: &TableState,
        path: &str,
        signals: Option<TableSignals>,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
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
            self.render_delete_dialog(cx, state).await?
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
            // The one dialog every row control on this table opens; empty
            // without delete chrome, where no control renders one.
            let chrome = RowChrome {
                with_bulk,
                with_actions,
                header_colspan: self.columns.len()
                    + usize::from(with_bulk)
                    + usize::from(with_actions),
                cell_widths,
                actions_min,
                delete_dialog_id: delete_prefix
                    .as_deref()
                    .map(Self::delete_dialog_dom_id)
                    .unwrap_or_default(),
            };
            let rows = render_rows(cx, row_data, &chrome);
            view! {
                cx =>
                // The table-level layout is a static class: Tailwind sees the
                // literal, and the decision carries no per-column value. Each
                // column's width, which does, rides the `th`/`td` inline
                // `style`. Fixed layout is what stops a filter or a page change
                // from re-measuring the columns. The `min-width` is the sum of
                // those declared widths: with `w-full` the table never exceeds
                // its container on its own, so without the floor the wrapper's
                // `overflow-x-auto` never scrolls.
                table(
                    attrs: attributes! { class="table-fixed" style=(table_min_width.as_deref()) },
                    (head)
                    table_body(
                        #[key(row.key.as_str())]
                        for row in rows {
                            (row.view)
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
        // invocation's identity, this call site, and the list path, so it is
        // stable across reruns and distinct per list (every resource's list
        // runs the same shard), and the runtime keeps its value when the
        // declaration renders again. Reading it for the input's initial
        // value is what declares the dependency; the token itself is opaque.
        //
        // A static table renders no control at all: it has no shard to re-run,
        // its region is inert markup, and the client replaces it wholesale
        // So the control's presence *is* the page's answer to
        // "can this table refresh in place?".
        let revision_attrs = signals.as_ref().map(|_| {
            let revision = topcoat::runtime::signal(&cx.keyed(path), || "0".to_string());
            attributes! {
                cx =>
                type="hidden"
                value=(revision.get())
                :value=$(revision.get())
                @change=$(|e: Event| revision.set(e.target.value))
                data-table-revision=""
            }
        });

        // One chrome for both branches: search bar, filter bar, bulk bar,
        // warning, table body, pager, dialog, inside the frame the morph swaps.
        //
        // The search form and the bulk form share the first toolbar row, the
        // search at the start and the bulk control at the end; a table with
        // neither renders no row, only the bulk placeholder.
        let toolbar_row = show_search || with_bulk;
        let content = view! {
            cx =>
            if toolbar_row {
                <div class=(BAR_CLASS)>
                    if show_search {
                        (search_bar.expect("search bar built when enabled"))
                    }
                    (bulk_bar_view)
                </div>
            } else {
                (bulk_bar_view)
            }
            if show_filters {
                (filter_bar.expect("filter bar built when enabled"))
            }
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
        };
        Ok(table_frame(
            cx,
            false,
            self.framed,
            self.delete_prefix.as_deref(),
            content.boxed(),
        ))
    }
}

/// The table's two wrappers, shared by the loaded table and its skeleton so
/// the swap lands on the same shape: the `data-boundary` region the morph
/// swaps, and the root inside it — the table's card when `framed`, a plain
/// block inside the page's card otherwise. A loading skeleton marks both
/// `aria-busy`: the boundary so assistive tech sees the live region, the root
/// so the busy state reads on the table itself.
///
/// `name` is the table's delete prefix — its resource's list path — when it
/// has delete chrome: a page holding several tables (a record page's
/// relations) names each region, so a mutation's response swaps into the
/// table the mutation came from (`mutation-submit.js`).
pub(super) fn table_frame<'a>(
    cx: &'a Cx,
    busy: bool,
    framed: bool,
    name: Option<&str>,
    content: BoxView<'a>,
) -> BoxView<'a> {
    let busy = busy.then_some("true");
    let name = name.map(str::to_string);
    let root_class = if framed {
        TABLE_CARD_CLASS
    } else {
        TABLE_BARE_CLASS
    };
    view! {
        cx =>
        <div data-boundary="table" data-table=(name) aria-busy=(busy)>
            <div class=(root_class) data-table-root="" aria-busy=(busy)>(content)</div>
        </div>
    }
    .boxed()
}

#[cfg(test)]
pub(super) mod tests;
