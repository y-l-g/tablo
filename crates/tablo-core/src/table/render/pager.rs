//! Previous/Next pagination links from the executed page’s real cursors.

use tablo_ui::{
    pagination, pagination_content, pagination_item, pagination_next, pagination_previous,
};
use topcoat::{Result, context::Cx, view::*};

use super::{Frame, live_link};
use crate::table::state::{Cursor, TableSignals, TableState};

impl Frame<'_> {
    /// Render Previous/Next links from the executed page's real cursors, `next_cursor` and
    /// `prev_cursor`, preserving search and sort state.
    pub(super) async fn render_pager<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        next_cursor: Option<&str>,
        prev_cursor: Option<&str>,
        signals: &TableSignals,
    ) -> Result<Vec<BoxView<'a>>> {
        let next_href =
            next_cursor.map(|cursor| state.with_cursor(path, &Cursor::After(cursor.to_string())));
        let prev_href =
            prev_cursor.map(|cursor| state.with_cursor(path, &Cursor::Before(cursor.to_string())));
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
