//! The streamed placeholder the list shell swaps the table into.

use tablo_ui::{table, table_body, table_cell, table_row};
use topcoat::{Result, context::Cx, view::*};

use super::super::{super::state::TableState, NormalizedState, Table};

impl<M> Table<M> {
    /// The skeleton placeholder table — three pulsing rows under the real
    /// column header. This is the [`suspense`] fallback for tables whose rows
    /// stream in. Wrapped in the same `data-boundary` region as the real table
    /// so the markup shape matches when the swap arrives.
    /// Carries `aria-busy` while loading plus toolbar/pager pulse placeholders
    /// so the streamed chrome lands without a layout shift.
    pub async fn render_skeleton<'a>(&self, cx: &'a Cx) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model,
    {
        let state = TableState::from_cx(cx);
        // Same normalization as the table seams: the placeholder
        // header links must not echo an unknown `?group_by=`.
        self.render_skeleton_normalized(cx, &self.normalize_state(&state))
            .await
    }

    /// [`Self::render_skeleton`] with the state already normalized:
    /// the panel parses and normalizes once per request and renders the
    /// streamed placeholder from that same state.
    pub(crate) async fn render_skeleton_normalized<'a>(
        &self,
        cx: &'a Cx,
        state: &NormalizedState,
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
        let inner = view! {
            cx =>
            <div
                class="rounded-xl border border-border overflow-hidden"
                data-table-root=""
                aria-busy="true"
            >
                <div class="border-b border-border p-3" aria-hidden="true">
                    <div class="animate-pulse rounded-md bg-foreground/10 h-9 w-64"></div>
                </div>
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
            </div>
        };
        // The busy state rides on the morph boundary so assistive
        // tech sees the live region, not just the swapped root below it.
        Ok(view! { cx => <div data-boundary="table" aria-busy="true">(inner)</div> }.boxed())
    }
}
#[cfg(test)]
mod tests;
