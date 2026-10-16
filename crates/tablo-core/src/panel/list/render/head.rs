//! The column-header row and its sort links.

use tablo_ui::{icons, table_head, table_header, table_row};
use topcoat::{Result, context::Cx, icon::icon, runtime::Event, view::*};

use super::{CHROME_CELL_CLASS, Frame, live_link};
use crate::{
    panel::list::TableSignals,
    table::{TableState, bulk_token},
};

impl Frame<'_> {
    /// Render the column-header row: sort links that write the table's query, and the
    /// select-all box over the page's selectable `keys`. Without `links` (the skeleton) the headers
    /// are plain labels.
    pub(super) async fn render_thead<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        with_actions: bool,
        with_bulk: bool,
        links: Option<(&TableSignals, Vec<String>)>,
    ) -> Result<BoxView<'a>> {
        let active = state.sort.as_ref().filter(|s| {
            self.columns
                .iter()
                .any(|c| c.sortable && c.name == s.column)
        });
        let mut heads: Vec<BoxView<'_>> = Vec::with_capacity(self.columns.len());
        let slack = self.slack_column();
        for (index, col) in self.columns.iter().enumerate() {
            let width = col.width.style();
            let fit = if slack == Some(index) { "w-full" } else { "" };
            let align = col.alignment.class();
            let label = col.label.to_string();
            let sortable = col.sortable;
            let (head_class, aria_sort, header) =
                if let (true, Some((signals, _))) = (sortable, links.as_ref()) {
                    let (aria, sort_icon, next_desc) = match active {
                        Some(s) if s.column == col.name => (
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
                    let href = state.sorted_by(path, col.name, next_desc);
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
                            class=([head_class, fit, align]
                                .join(" ")
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" "))
                            aria-sort=(aria_sort)
                            style=(width.as_deref())
                        },
                        (header)
                    )
                }
                .boxed(),
            );
        }
        let select_all = with_bulk.then(|| match &links {
            Some((signals, keys)) => select_all(cx, signals, keys),
            None => attributes! { cx => type="checkbox" disabled="" },
        });
        if with_actions {
            heads.push(
                view! {
                    cx =>
                    table_head(
                        attrs: attributes! { class=(CHROME_CELL_CLASS) },
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
                    if let Some(attrs) = select_all {
                        table_head(
                            attrs: attributes! { class=(CHROME_CELL_CLASS) },
                            <input aria-label="Select all rows" (attrs)>
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

/// The select-all box over a page's selectable `keys`: checked when the selection holds every one,
/// indeterminate when it holds some, and toggling all of them at once. Keys on other pages stay
/// selected.
fn select_all(cx: &Cx, signals: &TableSignals, keys: &[String]) -> Attributes {
    let bulk = signals.bulk.clone();
    // The tokens, never the raw keys: a key may hold the wire's separator, so only the renderer
    // can spell one (`state::bulk_token`).
    let tokens: Vec<String> = keys.iter().map(|key| bulk_token(key)).collect();
    // The handler reads them from one `|`-joined string: `${..}` hands JavaScript a hydratable
    // value, and a `Vec` hydrates to the runtime's `Vec` class, which holds none of the `Array`
    // methods below. A token holds no `|`, which the codec percent-encodes.
    let joined = tokens.join("|");
    attributes! {
        cx =>
        type="checkbox"
        :checked=$({
            let wire = bulk.get();
            raw!(
                "cx.hydrate((t => t.length > 0 && t.every(k => String(${wire}).includes(k)))(String(${joined}).split('|').filter(Boolean)))",
                !tokens.is_empty()
                    && tokens.iter().all(|token| wire.contains(token.as_str())),
            )
        })
        :indeterminate=$({
            let wire = bulk.get();
            raw!(
                "cx.hydrate((t => t.some(k => String(${wire}).includes(k)) && !t.every(k => String(${wire}).includes(k)))(String(${joined}).split('|').filter(Boolean)))",
                tokens.iter().any(|token| wire.contains(token.as_str()))
                    && !tokens.iter().all(|token| wire.contains(token.as_str())),
            )
        })
        @change=$(|e: Event| {
            let wire = bulk.get();
            if e.target.checked {
                bulk.set(
                    raw!(
                        "cx.hydrate((w => String(${joined}).split('|').filter(Boolean).filter(k => !w.includes(k)).reduce((s, k) => s + k, w))(String(${wire}).replace(/^,+$/, '')))",
                        wire.to_owned(),
                    ),
                );
            } else {
                bulk.set(
                    raw!(
                        "cx.hydrate(String(${joined}).split('|').filter(Boolean).reduce((s, k) => s.replaceAll(k, ','), String(${wire})).replace(/^,+$/, ''))",
                        wire.to_owned(),
                    ),
                );
            }
        })
    }
}
