//! The table's entry points and its card.

use tablo_ui::{table, table_body};
use toasty::stmt::{List, Query};
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::{TableSignals, WiredTable},
    Frame, TABLE_CARD_CLASS,
    filter_menu::FilterViews,
    head::Heads,
    pager::PagerInput,
    rows::{CellLayout, RowChrome, RowView, render_rows, selectable_keys},
    table_dom_id,
    tabs::{TabView, render_tabs},
};
use crate::{
    BoxFuture,
    table::{Sort, TablePage, TableState},
};

impl<M> WiredTable<M> {
    /// Load the page of `query` the table's browser state selects, and render it.
    ///
    /// Call it while a page body runs: the table declares its signals here, and the page reads
    /// the list state from them, so a sort, search, filter or page change in the browser reruns
    /// the page in place. Links keep their `href`, which spells the same state. A page rendering
    /// several tables gives each its own [`prefix`](Self::prefixed).
    ///
    /// # Panics
    ///
    /// Without a request in `cx`.
    ///
    /// # Errors
    ///
    /// A misdeclared table ([`Table::declaration_errors`](crate::table::Table::declaration_errors))
    /// fails with its errors rather than render, and a failed load fails with the database's
    /// error.
    pub async fn render<'a>(&self, cx: &'a Cx, query: Query<List<M>>) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        let path = topcoat::router::request::uri(cx).path().to_string();
        let (signals, state) = self.browser_state(cx);
        let page = TablePage::load(cx, self, query, &state).await?;
        self.render_page(cx, page, &state, &path, &signals).await
    }

    /// Render a loaded page for `state`, its links pointing at `path` and writing `signals`.
    pub(crate) async fn render_page<'a>(
        &self,
        cx: &'a Cx,
        page: TablePage<M>,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        let errors = self.declaration_errors();
        if !errors.is_empty() {
            return Err(crate::error::misdeclared(&errors));
        }
        let frame = self.frame();
        let group_key = self.effective_group_key(state);
        let rows = self.row_views(cx, &frame, &page, group_key.as_ref());
        let active = self.active_tab(state).map(|tab| tab.key());
        let tabs = self
            .declared_tabs()
            .iter()
            .map(|tab| {
                let count = page
                    .tab_counts
                    .iter()
                    .find(|(key, _)| key == tab.key())
                    .map(|(_, count)| *count);
                TabView::new(
                    tab.key(),
                    tab.label().localize(cx),
                    active == Some(tab.key()),
                    count,
                )
            })
            .collect();
        let loaded = Loaded {
            tabs,
            rows,
            next_cursor: page.next_cursor,
            prev_cursor: page.prev_cursor,
            position: page.position,
            sort: self.sort_for(state),
            filters: self.filter_views(cx, state).await?,
        };
        frame.render_page(cx, loaded, state, path, signals).await
    }
}

impl<M> WiredTable<M> {
    /// Render `rows` without the list's chrome: no toolbar, sort link, pager or column picker,
    /// and no write. Each row links its View and Edit as the table's wiring allows, and the
    /// zero-rows message stands in for none.
    ///
    /// # Errors
    ///
    /// A misdeclared table fails with its errors rather than render.
    pub(crate) async fn render_records<'a>(&self, cx: &'a Cx, rows: Vec<M>) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        let errors = self.declaration_errors();
        if !errors.is_empty() {
            return Err(crate::error::misdeclared(&errors));
        }
        let frame = self.frame();
        let state = TableState {
            prefix: self.prefix().map(str::to_string),
            ..TableState::default()
        };
        let signals = TableSignals::new(cx, state.prefix.as_deref());
        let rows = self.row_views(cx, &frame, &TablePage::from(rows), None);
        frame.render_plain(cx, rows, state, signals).await
    }
}

/// One loaded page, projected for rendering.
struct Loaded<'a> {
    tabs: Vec<TabView>,
    rows: Vec<RowView<'a>>,
    next_cursor: Option<String>,
    prev_cursor: Option<String>,
    position: Option<crate::table::PagePosition>,
    /// The sort the rows are in.
    sort: Option<Sort>,
    filters: FilterViews<'a>,
}

impl Frame<'_> {
    /// Render `loaded` for `state`, its links pointing at `path` and writing `signals`.
    ///
    /// Boxed so tablo-core compiles the render once. A future only a generic caller polls is
    /// compiled in each crate that instantiates the caller, that is, in every app.
    fn render_page<'f, 'a: 'f>(
        &'f self,
        cx: &'a Cx,
        loaded: Loaded<'a>,
        state: &'f TableState,
        path: &'f str,
        signals: &'f TableSignals,
    ) -> BoxFuture<'f, Result<BoxView<'a>>> {
        Box::pin(self.render_loaded(cx, loaded, state, path, signals))
    }

    /// Render `rows` as [`WiredTable::render_records`] says, boxed as [`Self::render_page`] is.
    fn render_plain<'f, 'a: 'f>(
        &'f self,
        cx: &'a Cx,
        rows: Vec<RowView<'a>>,
        state: TableState,
        signals: TableSignals,
    ) -> BoxFuture<'f, Result<BoxView<'a>>> {
        Box::pin(async move {
            let with_actions = self.with_actions();
            let head = self
                .render_thead(cx, &state, "", with_actions, false, Heads::Named)
                .await?;
            if rows.is_empty() {
                let empty = self
                    .render_empty_cell(cx, &state, "", with_actions, false, &signals)
                    .await?;
                return Ok(view! {
                    cx =>
                    table(
                        (head)
                        (empty)
                    )
                }
                .boxed());
            }
            let rows = render_rows(cx, rows, &self.row_chrome(&state, &signals, false));
            Ok(view! {
                cx =>
                table(
                    (head)
                    table_body(
                        #[key(row.key.as_str())]
                        for row in rows {
                            (row.view)
                        }
                    )
                )
            }
            .boxed())
        })
    }

    /// What each row of a render for `state` shares, wired to `signals`.
    fn row_chrome(&self, state: &TableState, signals: &TableSignals, with_bulk: bool) -> RowChrome {
        let with_actions = self.with_actions();
        RowChrome {
            with_bulk,
            with_actions,
            header_colspan: self.columns.len() + usize::from(with_bulk) + usize::from(with_actions),
            cells: self
                .columns
                .iter()
                .map(|col| CellLayout {
                    class: Some(col.alignment.class()).filter(|class| !class.is_empty()),
                    style: col.width.style(),
                    content: col.width.content_class(),
                })
                .collect(),
            signals: signals.clone(),
            form: table_dom_id(state, "writes"),
            action_menu: self.action_menu,
            id_scope: state.prefix.clone().unwrap_or_else(|| "table".to_string()),
        }
    }

    async fn render_loaded<'a>(
        &self,
        cx: &'a Cx,
        loaded: Loaded<'a>,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
    ) -> Result<BoxView<'a>> {
        let Loaded {
            tabs,
            rows: row_data,
            next_cursor,
            prev_cursor,
            position,
            sort,
            mut filters,
        } = loaded;
        let signals = &signals
            .clone()
            .with_inputs(cx, self.inputs.iter().map(|action| action.name));
        let with_actions = self.with_actions();
        let with_bulk = self.bulk_enabled();
        let head = self
            .render_thead(
                cx,
                state,
                path,
                with_actions,
                with_bulk,
                Heads::Linked(signals, sort.as_ref(), selectable_keys(&row_data)),
            )
            .await?;
        let tabs = render_tabs(cx, tabs, state, path, signals);
        let filter_warning = self.render_filter_warning(cx, state, path, signals, &filters);
        let controls = (std::mem::take(&mut filters.controls), filters.active);
        // Every record matching differs from the page's only when the list holds another page,
        // and a list ignoring a filter offers none: the route refuses what the list did not apply.
        let spans_pages = filters.unapplied.is_empty()
            && (next_cursor.is_some()
                || prev_cursor.is_some()
                || position.is_some_and(|position| position.has_next || position.page > 1));
        let toolbar = self
            .render_toolbar(cx, state, path, signals, controls, spans_pages)
            .await?;
        let pager = self
            .render_pager(
                cx,
                state,
                path,
                PagerInput {
                    next_cursor: next_cursor.as_deref(),
                    prev_cursor: prev_cursor.as_deref(),
                    position,
                    rows: row_data.len(),
                },
                signals,
            )
            .await?;
        let write_form = self.render_write_form(cx, state, signals);
        let input_dialogs = self.render_input_dialogs(cx, state, signals);

        let mut pager_views: Vec<BoxView<'_>> = Vec::new();
        let body: BoxView<'_> = if row_data.is_empty() {
            let empty_cell = self
                .render_empty_cell(cx, state, path, with_actions, with_bulk, signals)
                .await?;
            view! {
                cx =>
                // Alone in a box, the registry table draws no border: the bars beside it draw theirs.
                <div>
                    table(
                        (head)
                        (empty_cell)
                    )
                </div>
            }
            .boxed()
        } else {
            pager_views = pager;
            let rows = render_rows(cx, row_data, &self.row_chrome(state, signals, with_bulk));
            view! {
                cx =>
                <div>
                    table(
                        (head)
                        table_body(
                            #[key(row.key.as_str())]
                            for row in rows {
                                (row.view)
                            }
                        )
                    )
                </div>
            }
            .boxed()
        };

        let content = view! {
            cx =>
            if let Some(tabs) = tabs {
                (tabs)
            }
            (toolbar)
            if let Some(warning) = filter_warning {
                (warning)
            }
            (body)
            for p in pager_views {
                (p)
            }
            if let Some(form) = write_form {
                (form)
            }
            for dialog in input_dialogs {
                (dialog)
            }
        };
        Ok(table_frame(cx, false, content.boxed()))
    }
}

/// Render the table's card, shared by the loaded table and its skeleton.
pub(super) fn table_frame<'a>(cx: &'a Cx, busy: bool, content: BoxView<'a>) -> BoxView<'a> {
    let busy = busy.then_some("true");
    view! { cx => <div class=(TABLE_CARD_CLASS) aria-busy=(busy)>(content)</div> }.boxed()
}
