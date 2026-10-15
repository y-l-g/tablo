//! The pager under the rows: numbered page links with the result set's size, Previous and Next by
//! page number, or Previous and Next by the executed page's real cursors.

use tablo_ui::{
    pagination, pagination_content, pagination_ellipsis, pagination_item, pagination_link,
    pagination_next, pagination_previous,
};
use topcoat::{Result, context::Cx, view::*};

use super::{Frame, live_link, table_dom_id, toolbar::rerun_with};
use crate::table::{
    PagePosition,
    state::{Cursor, TableSignals, TableState},
};

/// The bar the pager renders in, under the rows.
const BAR_CLASS: &str =
    "flex flex-col items-center gap-3 border-t border-border px-4 py-3 sm:flex-row";

/// A Previous or Next link with nowhere to go.
const DISABLED_CLASS: &str = "pointer-events-none opacity-50";

/// The page links a pager shows between Previous and Next: the first and the last page, the
/// current one and its neighbors, `None` for each run of pages left out. A gap of one page shows
/// that page, since an ellipsis would take its place anyway.
pub(super) fn page_window(page: u32, last: u32) -> Vec<Option<u32>> {
    let mut shown: Vec<u32> = [1, page.saturating_sub(1), page, page + 1, last]
        .into_iter()
        .filter(|n| (1..=last).contains(n))
        .collect();
    shown.sort_unstable();
    shown.dedup();
    let mut window = Vec::with_capacity(shown.len() + 2);
    for (index, &n) in shown.iter().enumerate() {
        if let Some(&before) = index.checked_sub(1).and_then(|i| shown.get(i)) {
            match n - before {
                1 => {}
                2 => window.push(Some(before + 1)),
                _ => window.push(None),
            }
        }
        window.push(Some(n));
    }
    window
}

/// What the pager reads, about the page the table shows.
pub(super) struct PagerInput<'p> {
    pub(super) next_cursor: Option<&'p str>,
    pub(super) prev_cursor: Option<&'p str>,
    pub(super) position: Option<PagePosition>,
    /// The rows this page shows.
    pub(super) rows: usize,
}

impl Frame<'_> {
    /// Render the pager for the page `input` describes, its links keeping the search, the
    /// filters, the sort and the page size. A cursor page renders Previous and Next from its real
    /// cursors; a numbered one, its position.
    pub(super) async fn render_pager<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        input: PagerInput<'_>,
        signals: &TableSignals,
    ) -> Result<Vec<BoxView<'a>>> {
        match input.position {
            Some(position) => Ok(self
                .render_numbered(cx, state, path, position, input.rows, signals)
                .into_iter()
                .collect()),
            None => Ok(render_cursored(cx, state, path, &input, signals)
                .into_iter()
                .collect()),
        }
    }

    /// The numbered pager: the summary, the page-size choice, and the page links.
    fn render_numbered<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        position: PagePosition,
        rows: usize,
        signals: &TableSignals,
    ) -> Option<BoxView<'a>> {
        let PagePosition {
            page,
            per_page,
            total,
            has_next,
        } = position;
        let first = u64::from(page - 1) * u64::try_from(per_page).unwrap_or(u64::MAX) + 1;
        let last_row = first + u64::try_from(rows).unwrap_or(0).saturating_sub(1);
        let summary: BoxView<'a> = match total {
            Some(total) => view! {
                cx =>
                "Showing "
                <span class="font-medium text-foreground">
                    (format!("{first}–{last_row}"))
                </span>
                " of "
                <span class="font-medium text-foreground">(total.to_string())</span>
            }
            .boxed(),
            None => view! {
                cx =>
                "Page "
                <span class="font-medium text-foreground">(page.to_string())</span>
            }
            .boxed(),
        };
        let smallest = self.page_sizes.first().copied().unwrap_or(usize::MAX);
        let more_than_a_page = page > 1 || has_next;
        let sizes = (!self.page_sizes.is_empty()
            && (more_than_a_page
                || total.is_some_and(|total| total > u64::try_from(smallest).unwrap_or(u64::MAX))
                || per_page != smallest))
            .then(|| self.render_page_sizes(cx, state, path, per_page, signals));
        let link = |target: u32| live_link(cx, state.with_page(path, target), signals);
        let step = |target: u32, rel: &'static str| {
            let mut attrs = link(target);
            attrs.extend(attributes! { cx => rel=(rel) });
            attrs
        };
        let nav = more_than_a_page.then(|| {
            let previous = if page > 1 {
                step(page - 1, "prev")
            } else {
                disabled(cx)
            };
            let next = if has_next {
                step(page + 1, "next")
            } else {
                disabled(cx)
            };
            let numbers: Vec<BoxView<'a>> = position
                .last_page()
                .map(|last| page_window(page, last))
                .unwrap_or_default()
                .into_iter()
                .map(|entry| match entry {
                    Some(n) => {
                        let attrs = link(n);
                        let label = n.to_string();
                        view! {
                            cx =>
                            pagination_item(
                                attrs: attributes! { class="max-sm:hidden" },
                                pagination_link(active: n == page, attrs: attrs, (label))
                            )
                        }
                        .boxed()
                    }
                    None => view! {
                        cx =>
                        pagination_item(
                            attrs: attributes! { class="max-sm:hidden" },
                            pagination_ellipsis()
                        )
                    }
                    .boxed(),
                })
                .collect();
            view! {
                cx =>
                // A size container: its width must come from the bar, never its links.
                pagination(
                    attrs: attributes! { class="sm:flex-1 sm:justify-end" },
                    pagination_content(
                        pagination_item(pagination_previous(attrs: previous))
                        for number in numbers {
                            (number)
                        }
                        pagination_item(pagination_next(attrs: next))
                    )
                )
            }
            .boxed()
        });
        Some(
            view! {
                cx =>
                <div class=(BAR_CLASS) data-pager="">
                    <p class="shrink-0 text-sm text-muted-foreground tabular-nums">
                        (summary)
                    </p>
                    if let Some(sizes) = sizes {
                        (sizes)
                    }
                    if let Some(nav) = nav {
                        (nav)
                    }
                </div>
            }
            .boxed(),
        )
    }

    /// The page-size choice: a GET form carrying the rest of the state, which reruns the page at
    /// its first page when the size changes.
    fn render_page_sizes<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        per_page: usize,
        signals: &TableSignals,
    ) -> BoxView<'a> {
        let form = table_dom_id(state, "per-page");
        let select_id = format!("{form}-select");
        let name = state.param("per_page");
        let hidden: Vec<(String, String)> = state.hidden_pairs("per_page");
        let options: Vec<(String, bool)> = self
            .page_sizes
            .iter()
            .map(|size| (size.to_string(), *size == per_page))
            .collect();
        let (on_change, on_submit) = (rerun_with(signals, &form), rerun_with(signals, &form));
        let action = path.to_string();
        view! {
            cx =>
            <form
                id=(form)
                method="get"
                action=(action)
                class="flex items-center gap-2 text-sm text-muted-foreground"
                @change=(on_change)
                @submit=(on_submit)
            >
                for (key, value) in hidden {
                    <input type="hidden" name=(key) value=(value)>
                }
                <label for=(select_id.clone()) class="shrink-0">"Per page"</label>
                tablo_ui::select(
                    attrs: attributes! { id=(select_id) name=(name) class="w-20" },
                    for (value, selected) in options {
                        <option value=(value.clone()) selected=(selected)>
                            (value)
                        </option>
                    }
                )
                <button type="submit" class="sr-only">"Apply"</button>
            </form>
        }
        .boxed()
    }
}

/// Previous and Next from a cursor page's real cursors; nothing when it has neither.
fn render_cursored<'a>(
    cx: &'a Cx,
    state: &TableState,
    path: &str,
    input: &PagerInput<'_>,
    signals: &TableSignals,
) -> Option<BoxView<'a>> {
    let next_href = input
        .next_cursor
        .map(|cursor| state.with_cursor(path, &Cursor::After(cursor.to_string())));
    let prev_href = input
        .prev_cursor
        .map(|cursor| state.with_cursor(path, &Cursor::Before(cursor.to_string())));
    if prev_href.is_none() && next_href.is_none() {
        return None;
    }
    let step = |href: String, rel: &'static str| {
        let mut attrs = live_link(cx, href, signals);
        attrs.extend(attributes! { cx => rel=(rel) });
        attrs
    };
    let previous = prev_href.map_or_else(|| disabled(cx), |href| step(href, "prev"));
    let next = next_href.map_or_else(|| disabled(cx), |href| step(href, "next"));
    Some(
        view! {
            cx =>
            <div class=(BAR_CLASS) data-pager="">
                pagination(
                    attrs: attributes! { class="sm:justify-end" },
                    pagination_content(
                        pagination_item(pagination_previous(attrs: previous))
                        pagination_item(pagination_next(attrs: next))
                    )
                )
            </div>
        }
        .boxed(),
    )
}

/// The attributes of a Previous or Next link with nowhere to go: no `href`, so it is no link.
fn disabled(cx: &Cx) -> Attributes {
    attributes! { cx => aria-disabled="true" class=(DISABLED_CLASS) }
}

#[cfg(test)]
mod tests;
