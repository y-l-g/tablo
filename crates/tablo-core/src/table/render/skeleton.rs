//! The placeholder the list page shows while its rows load.

use tablo_ui::{table, table_body, table_cell, table_row};
use topcoat::{Result, context::Cx, view::*};

use super::{super::WiredTable, Frame, core::table_frame};
use crate::table::state::TableState;

impl<M> WiredTable<M> {
    /// Render the skeleton placeholder table shown while rows load.
    pub(crate) async fn render_skeleton<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
    ) -> Result<BoxView<'a>>
    where
        M: toasty::schema::Model,
    {
        self.frame().render_skeleton(cx, state).await
    }
}

impl Frame<'_> {
    async fn render_skeleton<'a>(&self, cx: &'a Cx, state: &TableState) -> Result<BoxView<'a>> {
        let with_actions = self.with_actions();
        let with_bulk = self.bulk_enabled();
        let head = self
            .render_thead(cx, state, "", with_actions, with_bulk, None)
            .await?;
        let table_min_width = self.column_widths().table_min_width;
        let column_count = self.columns.len();
        let search_pulse = self.search;
        let filter_pulse = self.filter_bar;
        let content = view! {
            cx =>
            if search_pulse || with_bulk {
                <div
                    class="flex items-center gap-2 border-b border-border p-3"
                    aria-hidden="true"
                >
                    if search_pulse {
                        <div class="animate-pulse rounded-md bg-foreground/10 h-9 w-72"></div>
                    }
                    if with_bulk {
                        <div
                            class="ml-auto animate-pulse rounded-md bg-foreground/10 h-9 w-36"
                        ></div>
                    }
                </div>
            }
            if filter_pulse {
                <div class="border-b border-border p-3" aria-hidden="true">
                    <div class="animate-pulse rounded-md bg-foreground/10 h-9 w-96"></div>
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
