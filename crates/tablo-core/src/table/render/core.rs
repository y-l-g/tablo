//! The table's entry points and its card.

use tablo_ui::{table, table_body};
use toasty::stmt::{List, Query};
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::WiredTable,
    Frame, TABLE_CARD_CLASS,
    filterbar::FilterViews,
    rows::{RowChrome, RowView, render_rows, selectable_keys},
    table_dom_id,
    widths::ColumnWidths,
};
use crate::table::{
    page::TablePage,
    state::{TableSignals, TableState},
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
    /// Without a request context in `cx`.
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
        let loaded = Loaded {
            rows,
            next_cursor: page.next_cursor,
            prev_cursor: page.prev_cursor,
            filters: self.filter_views(cx, state),
        };
        frame.render_page(cx, loaded, state, path, signals).await
    }
}

/// One loaded page, projected for rendering.
struct Loaded<'a> {
    rows: Vec<RowView<'a>>,
    next_cursor: Option<String>,
    prev_cursor: Option<String>,
    filters: FilterViews<'a>,
}

impl Frame<'_> {
    /// Render `loaded` for `state`, its links pointing at `path` and writing `signals`.
    async fn render_page<'a>(
        &self,
        cx: &'a Cx,
        loaded: Loaded<'a>,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
    ) -> Result<BoxView<'a>> {
        let Loaded {
            rows: row_data,
            next_cursor,
            prev_cursor,
            mut filters,
        } = loaded;
        let with_actions = self.with_actions();
        let with_bulk = self.bulk_enabled();
        let head = self
            .render_thead(
                cx,
                state,
                path,
                with_actions,
                with_bulk,
                Some((signals, selectable_keys(&row_data))),
            )
            .await?;
        let filter_warning = self.render_filter_warning(cx, state, path, signals, &filters);
        let controls = std::mem::take(&mut filters.controls);
        let toolbar = self
            .render_toolbar(cx, state, path, signals, controls)
            .await?;
        let pager = self
            .render_pager(
                cx,
                state,
                path,
                next_cursor.as_deref(),
                prev_cursor.as_deref(),
                signals,
            )
            .await?;
        let write_form = self.render_write_form(cx, state, signals);

        let ColumnWidths {
            cells: cell_widths,
            actions_min,
            table_min_width,
            ..
        } = self.column_widths();
        let mut pager_views: Vec<BoxView<'_>> = Vec::new();
        let body: BoxView<'_> = if row_data.is_empty() {
            let empty_cell = self
                .render_empty_cell(cx, state, path, with_actions, with_bulk, signals)
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
            let chrome = RowChrome {
                with_bulk,
                with_actions,
                header_colspan: self.columns.len()
                    + usize::from(with_bulk)
                    + usize::from(with_actions),
                cell_widths,
                actions_min,
                signals: signals.clone(),
                form: table_dom_id(state, "writes"),
            };
            let rows = render_rows(cx, row_data, &chrome);
            view! {
                cx =>
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

        let content = view! {
            cx =>
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
        };
        Ok(table_frame(cx, false, content.boxed()))
    }
}

#[cfg(test)]
impl<M> crate::table::Table<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    /// This table, wired with nothing, rendering `page` as [`WiredTable::render_loaded`] does.
    pub(crate) async fn render<'a>(&self, cx: &'a Cx, page: TablePage<M>) -> Result<BoxView<'a>> {
        self.clone()
            .wired()
            .render_loaded_with(cx, page, &TableState::default(), "")
            .await
    }

    /// [`Self::render`] for `state`, its links pointing at `path`.
    pub(crate) async fn render_with_state<'a>(
        &self,
        cx: &'a Cx,
        page: TablePage<M>,
        state: &TableState,
        path: &str,
    ) -> Result<BoxView<'a>> {
        self.clone()
            .wired()
            .render_loaded_with(cx, page, state, path)
            .await
    }
}

#[cfg(test)]
impl<M> WiredTable<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    /// Render a loaded `page` with neutral state inside a page body, as a test's view resolves it.
    pub(crate) async fn render_loaded<'a>(
        self,
        cx: &'a Cx,
        page: TablePage<M>,
    ) -> Result<BoxView<'a>> {
        self.render_loaded_with(cx, page, &TableState::default(), "")
            .await
    }

    /// [`Self::render_loaded`] for `state`, its links pointing at `path`.
    pub(crate) async fn render_loaded_with<'a>(
        self,
        cx: &'a Cx,
        page: TablePage<M>,
        state: &TableState,
        path: &str,
    ) -> Result<BoxView<'a>> {
        let errors = self.declaration_errors();
        if !errors.is_empty() {
            return Err(crate::error::misdeclared(&errors));
        }
        let (state, path) = (state.clone(), path.to_string());
        Ok(crate::topcoat_compat::async_page(async move {
            let signals = TableSignals::new(cx, state.prefix.as_deref());
            let state = self.normalize_state(&state);
            self.render_page(cx, page, &state, &path, &signals).await
        }))
    }
}

/// Render the table's card, shared by the loaded table and its skeleton.
pub(super) fn table_frame<'a>(cx: &'a Cx, busy: bool, content: BoxView<'a>) -> BoxView<'a> {
    let busy = busy.then_some("true");
    view! { cx => <div class=(TABLE_CARD_CLASS) aria-busy=(busy)>(content)</div> }.boxed()
}

#[cfg(test)]
pub(super) mod tests;
