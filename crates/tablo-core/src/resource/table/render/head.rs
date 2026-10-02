//! The column-header row and its sort links.

use tablo_ui::{icons, table_head, table_header, table_row};
use topcoat::{Result, context::Cx, icon::icon, view::*};

use super::{
    super::{
        super::state::{TableSignals, TableState},
        Table,
    },
    live_link,
};

impl<M> Table<M> {
    /// Render the shared column-header row with sort links on sortable columns, writing the query signal on live tables.
    pub(super) async fn render_thead<'a>(
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
        let active = state.sort.as_ref().filter(|s| {
            self.columns
                .iter()
                .any(|c| c.is_sortable() && c.name() == s.column)
        });
        let widths = self.column_widths();
        let mut heads: Vec<BoxView<'_>> = Vec::with_capacity(self.columns.len());
        for (index, col) in self.columns.iter().enumerate() {
            let width = widths.cells[index].clone();
            let label = col.label().to_string();
            let sortable = col.is_sortable();
            let (head_class, aria_sort, header) = if sortable {
                let (aria, sort_icon, next_desc) = match active {
                    Some(s) if s.column == col.name() => (
                        if s.descending {
                            "descending"
                        } else {
                            "ascending"
                        },
                        if s.descending {
                            icons::ARROW_DOWN
                        } else {
                            icons::ARROW_UP
                        },
                        !s.descending,
                    ),
                    _ => ("none", icons::ARROW_UP_DOWN, false),
                };
                let href = state.sorted_by(path, col.name(), next_desc);
                let aria_label = format!(
                    "Sort by {} {}",
                    label,
                    if next_desc { "descending" } else { "ascending" }
                );
                let link_attrs = live_link(cx, href, signals);
                (
                    "cursor-pointer hover:bg-foreground/5",
                    Some(aria),
                    view! {
                        cx =>
                        <a
                            class="inline-flex items-center gap-1 hover:text-foreground"
                            aria-label=(aria_label)
                            (link_attrs)
                        >
                            (label.clone())
                            icon(
                                data: sort_icon,
                                attrs: attributes! { class="size-4 shrink-0 text-muted-foreground" }
                            )
                        </a>
                    }
                    .boxed(),
                )
            } else {
                ("", None, view! { cx => (label.clone()) }.boxed())
            };
            heads.push(
                view! {
                    cx =>
                    table_head(
                        attrs: attributes! {
                            class=(head_class)
                            aria-sort=(aria_sort)
                            style=(width.as_deref())
                        },
                        (header)
                    )
                }
                .boxed(),
            );
        }
        if with_actions {
            heads.push(
                view! {
                    cx =>
                    table_head(
                        attrs: attributes! { style=(widths.actions.as_deref()) },
                        <span class="sr-only">"Actions"</span>
                    )
                }
                .boxed(),
            );
        }
        Ok(view! {
            cx =>
            table_header(
                table_row(
                    if with_bulk {
                        table_head(
                            attrs: attributes! { style=(widths.bulk.as_deref()) },
                            <input
                                type="checkbox"
                                aria-label="Select all rows"
                                data-bulk-select-all=""
                            >
                        )
                    }
                    for h in heads {
                        (h)
                    }
                )
            )
        }
        .boxed())
    }
}
