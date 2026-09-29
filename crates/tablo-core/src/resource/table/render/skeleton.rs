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
mod tests {
    use topcoat::context::CxTestBuilder;

    use super::{
        super::core::tests::{User, normalized_table_tag, table_tag},
        *,
    };
    use crate::{TableState, TextColumn};

    #[tokio::test]
    async fn skeleton_shares_the_table_root_with_the_swapped_body() {
        let cx = CxTestBuilder::new().build();
        let tbl = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        );
        let html = tbl
            .render_skeleton(&cx)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains("data-table-root"),
            "skeleton must share table root, got {html}"
        );
        assert!(
            html.contains("aria-busy"),
            "skeleton must announce loading, got {html}"
        );
        assert_eq!(
            html.matches("aria-busy=\"true\"").count(),
            2,
            "busy must ride on the morph boundary and the table root (GH #160), got {html}"
        );
        assert!(
            html.contains("aria-hidden"),
            "skeleton must hold chrome placeholders, got {html}"
        );
        // The skeleton and the swapped table must declare the same layout, or
        // the swap re-measures the columns: comparing the two
        // opening tags states that without pinning a class literal.
        let skeleton_table = table_tag(&html).to_string();
        // The swap payload is the table itself, under the same boundary region.
        let rows = vec![User {
            id: uuid::Uuid::nil(),
            name: "Ada".to_string(),
        }];
        let html = tbl
            .render_with_state(&cx, rows.into(), &TableState::default(), "/admin/users")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            html.contains("data-table-root") && html.contains("data-boundary=\"table\""),
            "the swapped table must land in the skeleton's region, got {html}"
        );
        assert!(
            html.contains("Ada"),
            "swap payload must be rows, got {html}"
        );
        // Attribute order is a serializer detail: `table` merges its own
        // classes with the caller's `attrs`, so the skeleton may emit
        // `style` before `class` while the swapped table emits them the
        // other way round. What matters for GH #240 is the same layout —
        // the same classes and the same floor — not the same byte order.
        let swapped_tag = table_tag(&html);
        assert!(
            swapped_tag.contains("table-fixed") && skeleton_table.contains("table-fixed"),
            "the swapped table must declare the skeleton's layout (GH #240), got {html}"
        );
        assert_eq!(
            normalized_table_tag(swapped_tag),
            normalized_table_tag(&skeleton_table),
            "the swapped table must declare the skeleton's layout (GH #240), got {html}"
        );
    }

    #[tokio::test]
    async fn skeleton_carries_the_action_column_for_view_only_chrome() {
        // The skeleton's action column must count every row link `render_inner`
        // renders, `with_view` included, or the swap changes the table width.
        let cx = CxTestBuilder::new().build();
        let tbl = Table::<User>::new(
            |u| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u| u.name.clone()),
        )
        .with_view("/admin/users".to_string());
        let skeleton = tbl
            .render_skeleton(&cx)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        let rows = vec![User {
            id: uuid::Uuid::nil(),
            name: "Ada".to_string(),
        }];
        let rendered = tbl
            .render_with_state(&cx, rows.into(), &TableState::default(), "/admin/users")
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert_eq!(
            rendered.matches(">Actions</th>").count(),
            1,
            "the real table renders one action column, got {rendered}"
        );
        assert_eq!(
            skeleton.matches(">Actions</th>").count(),
            rendered.matches(">Actions</th>").count(),
            "the skeleton must match the swapped table's column count, got {skeleton}"
        );
    }
}
