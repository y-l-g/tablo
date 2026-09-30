//! Previous/Next pagination links from the executed page’s real cursors.

use tablo_ui::{
    pagination, pagination_content, pagination_item, pagination_next, pagination_previous,
};
use topcoat::{Result, context::Cx, runtime::Event, view::*};

use super::super::{
    super::state::{TablePage, TableSignals, TableState},
    Table,
};

impl<M> Table<M> {
    /// Previous/Next pagination links from the executed page's real cursors.
    /// Empty when the page has no neighbors —
    /// no invented page numbers. Links preserve the search and sort state;
    /// cursors travel via `?after=`/`?before=`.
    ///
    /// With `signals` (a live table) each link also writes its cursor signal
    /// and clears the opposite one; `href` stays the no-JS fallback.
    pub(super) async fn render_pager<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        page: &TablePage<M>,
        signals: Option<&TableSignals>,
    ) -> Result<Vec<BoxView<'a>>> {
        // Cursors only carry ordering values; the loader re-applies search and
        // sort, so the links must carry that state along.
        let next_href = page
            .next_cursor
            .as_ref()
            .map(|cursor| state.with_after(path, cursor));
        let prev_href = page
            .prev_cursor
            .as_ref()
            .map(|cursor| state.with_before(path, cursor));
        if prev_href.is_none() && next_href.is_none() {
            return Ok(Vec::new());
        }
        let prev_item: Option<BoxView<'a>> = prev_href.map(|href| {
            let attrs = match (signals, page.prev_cursor.as_deref()) {
                (Some(signals), Some(cursor)) => {
                    let wire = crate::resource::cursor_before(cursor);
                    let signal = signals.cursor.clone();
                    attributes! {
                        cx =>
                        href=(href.clone())
                        @click=$(|e: Event| {
                            e.prevent_default();
                            signal.set(wire.clone());
                        })
                    }
                }
                _ => attributes! { cx => href=(href) },
            };
            view! { cx => pagination_item(pagination_previous(attrs: attrs)) }.boxed()
        });
        let next_item: Option<BoxView<'a>> = next_href.map(|href| {
            let attrs = match (signals, page.next_cursor.as_deref()) {
                (Some(signals), Some(cursor)) => {
                    let wire = crate::resource::cursor_after(cursor);
                    let signal = signals.cursor.clone();
                    attributes! {
                        cx =>
                        href=(href.clone())
                        @click=$(|e: Event| {
                            e.prevent_default();
                            signal.set(wire.clone());
                        })
                    }
                }
                _ => attributes! { cx => href=(href) },
            };
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
