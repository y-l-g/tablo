//! Previous/Next pagination links from the executed page’s real cursors.

use tablo_ui::{
    pagination, pagination_content, pagination_item, pagination_next, pagination_previous,
};
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::{
        super::{
            page::TablePage,
            state::{Cursor, TableSignals, TableState},
        },
        Table,
    },
    live_link,
};

impl<M> Table<M> {
    /// Render Previous/Next links from the executed page's real cursors, preserving search and sort
    /// state.
    pub(super) async fn render_pager<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        page: &TablePage<M>,
        signals: Option<&TableSignals>,
    ) -> Result<Vec<BoxView<'a>>> {
        let next_href = page
            .next_cursor
            .as_ref()
            .map(|cursor| state.with_cursor(path, &Cursor::After(cursor.clone())));
        let prev_href = page
            .prev_cursor
            .as_ref()
            .map(|cursor| state.with_cursor(path, &Cursor::Before(cursor.clone())));
        if prev_href.is_none() && next_href.is_none() {
            return Ok(Vec::new());
        }
        let prev_item: Option<BoxView<'a>> = prev_href.map(|href| {
            let attrs = live_link(cx, href, signals);
            view! { cx => pagination_item(pagination_previous(attrs: attrs)) }.boxed()
        });
        let next_item: Option<BoxView<'a>> = next_href.map(|href| {
            let attrs = live_link(cx, href, signals);
            view! { cx => pagination_item(pagination_next(attrs: attrs)) }.boxed()
        });
        let pager = view! {
            cx =>
            <div class="border-t border-border p-3">
                pagination(
                    pagination_content(
                        if let Some(item) = prev_item {
                            (item)
                        }
                        if let Some(item) = next_item {
                            (item)
                        }
                    )
                )
            </div>
        };
        Ok(vec![pager.boxed()])
    }
}
#[cfg(test)]
mod tests;
