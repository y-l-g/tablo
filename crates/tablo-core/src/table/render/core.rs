//! Static and live entry points and the table chrome they share.

use std::sync::Arc;

use tablo_ui::{table, table_body};
use topcoat::{Result, context::Cx, runtime::Event, view::*};

use super::{
    super::{Table, WiredTable},
    BAR_CLASS, TABLE_BARE_CLASS, TABLE_CARD_CLASS,
    rows::{RowChrome, render_rows},
    widths::ColumnWidths,
};
use crate::table::{
    page::TablePage,
    state::{TableSignals, TableState},
};

impl<M> Table<M> {
    /// Render the table for the given loaded page, with no row action, bulk bar, or custom
    /// action wired on: a page renders the panel's wiring through
    /// [`wired_table`](crate::panel::wired_table).
    ///
    /// # Errors
    ///
    /// A misdeclared table ([`Table::declaration_errors`]) fails with its
    /// errors rather than render.
    pub async fn render<'a>(&self, cx: &'a Cx, page: TablePage<M>) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model + Send + Sync + 'static,
    {
        WiredTable::new(Arc::new(self.clone()))
            .render(cx, page)
            .await
    }

    /// Render with explicit list state and path, with nothing wired on, as [`Self::render`].
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
        WiredTable::new(Arc::new(self.clone()))
            .render_with_state(cx, page, state, path)
            .await
    }
}

impl<M> WiredTable<M> {
    /// Render the table for the given loaded page.
    ///
    /// # Errors
    ///
    /// A misdeclared table ([`Table::declaration_errors`]) fails with its
    /// errors rather than render.
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

    /// Render with explicit list state and path, normalizing the state before rendering links.
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

    /// Render the interactive body for a live table, excluding the row-delete dialog the caller
    /// renders separately.
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
        let errors = self.declaration_errors();
        if !errors.is_empty() {
            return Err(crate::error::misdeclared(&errors));
        }
        let delete_prefix = self.delete_prefix().map(str::to_string);
        let with_actions = self.with_actions();
        let with_bulk = self.bulk_enabled();
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
        let group_key = self.effective_group_key(state);
        let row_data = self.row_views(cx, state, path, &page, group_key.as_ref());
        let delete_dialog = if signals.is_none() {
            self.render_delete_dialog(cx, state).await?
        } else {
            None
        };
        let action_dialog = if signals.is_none() {
            self.render_action_confirm_dialog(cx)
        } else {
            None
        };

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
                action_dialog_id: self
                    .actions_prefix()
                    .map(Self::action_confirm_dialog_dom_id)
                    .unwrap_or_default(),
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
            if let Some(dialog) = action_dialog {
                (dialog)
            }
        };
        Ok(table_frame(
            cx,
            false,
            self.framed(),
            self.delete_prefix(),
            content.boxed(),
        ))
    }
}

/// Render the table's two wrappers shared by the loaded table and its skeleton, naming the region
/// from the table's delete prefix.
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
