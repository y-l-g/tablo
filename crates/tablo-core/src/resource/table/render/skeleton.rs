//! The streamed placeholder the list shell swaps the table into.

use tablo_ui::{table, table_body, table_cell, table_row};
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::{super::state::TableState, Table},
    core::table_frame,
};

impl<M> Table<M> {
    /// The skeleton placeholder table — three pulsing rows under the real
    /// column header. This is the [`suspense`] fallback for tables whose rows
    /// stream in. Built in the same frame as the real table
    /// ([`table_frame`](super::core::table_frame)), busy while loading, with a
    /// pulse for each bar the loaded table renders, so the swap lands without
    /// a layout shift.
    ///
    /// Takes the state already normalized: the panel parses and normalizes
    /// once per request and renders the streamed placeholder from that same
    /// state, so the placeholder header links never echo an unknown
    /// `?group_by=`.
    pub(crate) async fn render_skeleton<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model,
    {
        let path = topcoat::context::try_request_context::<http::request::Parts>(cx)
            .map(|parts| parts.uri.path().to_string())
            .unwrap_or_default();
        // The action column exists for any of the three row links, matching
        // `render_inner` — a `with_view`-only table must not swap a
        // narrower skeleton for a wider table.
        let with_actions = self.with_actions();
        let with_bulk = self.bulk_enabled();
        let head = self
            .render_thead(cx, state, &path, with_actions, with_bulk, None)
            .await?;
        // The same floor the loaded table carries, so the swap lands without
        // a layout shift.
        let table_min_width = self.column_widths().table_min_width;
        let column_count = self.columns.len();
        // The chrome pulses follow the loaded table's own predicates, so a
        // table with no searchable column, no filters or no bulk delete shows
        // no pulse for a bar it will never render. The pager pulse always shows: every table
        // paginates, and whether this page has neighbors is only known once it
        // loads.
        let search_pulse = self.search_enabled();
        let filter_pulse = self.filter_bar_enabled();
        let content = view! {
            cx =>
            if search_pulse {
                <div class="border-b border-border p-3" aria-hidden="true">
                    <div class="animate-pulse rounded-md bg-foreground/10 h-9 w-64"></div>
                </div>
            }
            if filter_pulse {
                <div class="border-b border-border p-3" aria-hidden="true">
                    <div class="animate-pulse rounded-md bg-foreground/10 h-9 w-96"></div>
                </div>
            }
            if with_bulk {
                <div class="border-b border-border p-3" aria-hidden="true">
                    <div class="animate-pulse rounded-md bg-foreground/10 h-9 w-28"></div>
                </div>
            }
            table(
                attrs: attributes! { class="table-fixed" style=(table_min_width.as_deref()) },
                (head)
                table_body(
                    #[key(i)]
                    for i in 0..3 {
                        table_row(
                            if with_bulk {
                                table_cell(
                                    <div
                                        class="animate-pulse rounded-md bg-foreground/10 h-4 w-4"
                                    ></div>
                                )
                            }
                            for _ in 0..column_count {
                                table_cell(
                                    <div
                                        class="animate-pulse rounded-md bg-foreground/10 h-4 w-full"
                                    ></div>
                                )
                            }
                            if with_actions {
                                table_cell(
                                    <div
                                        class="animate-pulse rounded-md bg-foreground/10 h-4 w-12"
                                    ></div>
                                )
                            }
                        )
                    }
                )
            )
            <div class="border-t border-border p-3" aria-hidden="true">
                <div class="animate-pulse rounded-md bg-foreground/10 h-9 w-40"></div>
            </div>
        };
        Ok(table_frame(cx, true, content.boxed()))
    }
}
#[cfg(test)]
mod tests;
