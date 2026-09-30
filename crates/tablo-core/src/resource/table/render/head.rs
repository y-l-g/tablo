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
    /// The shared column-header row — the single source of the `<thead>`
    /// markup: labels and **links** on sortable columns that toggle
    /// `?sort=`/`?dir=` (a Lucide arrow with `aria-sort` when active,
    /// `arrow-up-down` when inactive). Every render branch (skeleton / empty
    /// / rows) composes it, so an a11y or styling change happens once.
    ///
    /// With `signals` (a live table) the link writes its own query, which drops
    /// the cursor, to the `query` signal; its `href` stays the no-JS fallback.
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
        // The active sort only counts when it names a declared sortable column.
        let active = state.sort.as_ref().filter(|s| {
            self.columns
                .iter()
                .any(|c| c.is_sortable() && c.name() == s.column)
        });
        let widths = self.column_widths();
        let mut heads: Vec<BoxView<'_>> = Vec::with_capacity(self.columns.len());
        for (index, col) in self.columns.iter().enumerate() {
            // Owned per iteration: the view must not borrow the resolved list.
            let width = widths.cells[index].clone();
            let label = col.label().to_string();
            // The declared width rides the header cell's inline `style`
            // a Tailwind class assembled at render would emit no
            // CSS, because Tailwind only generates the literals it finds in
            // source. A wide column declares nothing and takes a share of what
            // the declared columns leave.
            // A static preview renders plain labels: no link to an interaction
            // the page does not honor.
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
                        // toggling the active column flips the direction
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
                    // The share sizes the column at wide viewports; the floor
                    // sizes it to its buttons at narrow ones, where the share
                    // alone would let them spill past the table. Each row's
                    // `td` repeats only the floor.
                    table_head(
                        attrs: attributes! { style=(widths.actions.as_deref()) },
                        "Actions"
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
                        // The header row is the row `table-fixed` measures, so
                        // the chrome columns declare their width here and
                        // their `td`s declare none.
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
