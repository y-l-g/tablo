//! The zero-rows cell and its clear and back-to-first-page links.

use tablo_ui::{table_body, table_cell, table_row};
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::{
        super::state::{TableSignals, TableState},
        Table,
    },
    EMPTY_LINK_CLASS, live_link,
};

impl<M> Table<M> {
    /// Render the zero-rows cell spanning the table, with clear and back-to-first-page links when
    /// filtered.
    pub(super) async fn render_empty_cell<'a>(
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
        let clear_url = filtered.then(|| {
            if state.search.is_some() {
                state.without_search(path)
            } else {
                state.without_filters(path)
            }
        });
        let message = match &state.search {
            Some(term) => format!("No matches for \u{201c}{term}\u{201d}"),
            None if !state.filters.is_empty() => "No results for these filters".to_string(),
            None => "No records yet".to_string(),
        };
        let clears_search = state.search.is_some();
        let clear_label = if clears_search {
            "Clear search"
        } else {
            "Clear filters"
        };
        let first_page_url = state.cursor.is_some().then(|| state.without_cursor(path));
        let clear_link: Option<BoxView<'a>> = clear_url.map(|url| {
            let attrs = attributes! {
                cx =>
                href=(url)
                if clears_search {
                    data-search-clear=""
                }
                if !clears_search {
                    data-filters-clear=""
                }
            };
            view! { cx => <a class=(EMPTY_LINK_CLASS) (attrs)>(clear_label)</a> }.boxed()
        });
        let first_page_link: Option<BoxView<'a>> = first_page_url.map(|url| {
            let attrs = live_link(cx, url, signals);
            view! { cx => <a class=(EMPTY_LINK_CLASS) (attrs)>"Back to first page"</a> }.boxed()
        });
        let action: Option<Child<'a>> =
            (clear_link.is_some() || first_page_link.is_some()).then(|| {
                view! {
                    cx =>
                    if let Some(link) = clear_link {
                        (link)
                    }
                    if let Some(link) = first_page_link {
                        (link)
                    }
                }
                .boxed()
                .into()
            });
        Ok(view! {
            cx =>
            table_body(
                table_row(
                    table_cell(
                        attrs: attributes! { colspan=(colspan) class="whitespace-normal!" },
                        tablo_ui::empty_state(title: message, action: action)
                    )
                )
            )
        }
        .boxed())
    }
}
