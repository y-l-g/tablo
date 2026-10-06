//! [`WiredTable`](super::WiredTable) HTML rendering: the entry points plus the chrome.
//!
//! Grouping is page-local and interleaved and a page encodes its row-action
//! URL base once.
//!
//! The entry points and the table chrome live in `core`, the header row in
//! `head`, row projection and rendering in `rows`, the zero-rows cell in
//! `empty`, column widths in `widths`, the search and bulk bars in `toolbar`,
//! the filter bar in `filterbar`, pagination in `pager`, the confirmation
//! dialogs in `dialog`, and the streamed placeholder in `skeleton`.

mod core;
mod dialog;
mod empty;
mod filterbar;
mod head;
mod pager;
mod rows;
mod skeleton;
mod toolbar;
mod widths;

use topcoat::{
    context::Cx,
    runtime::Event,
    view::{Attributes, StaticClass, attributes, class},
};

use super::state::{TableSignals, TableState, query_of};

/// A toolbar row above the table: the search-and-bulk row and the filter bar.
const BAR_CLASS: StaticClass =
    class!("flex flex-wrap items-center gap-2 border-b border-border p-3");

/// The search input's box: its width, and the anchor for the icon inside it.
const SEARCH_FIELD_CLASS: StaticClass = class!("relative w-full sm:w-72");

/// The magnifier inside the search input's leading padding.
const SEARCH_ICON_CLASS: StaticClass = class!(
    "pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
);

/// A table's own card: the surface every panel card shares (`tablo_ui::card`'s
/// border, fill and shadow), clipped so the rows keep its rounded corners.
pub(crate) const TABLE_CARD_CLASS: StaticClass = class!(
    "overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-sm"
);

/// A secondary link inside a bar ("Clear filters", "Clear search").
const QUIET_LINK_CLASS: StaticClass = class!("text-sm text-muted-foreground hover:text-foreground");

/// A link in the empty-table message ("Clear search", "Back to first page").
const EMPTY_LINK_CLASS: StaticClass =
    class!("text-sm font-medium text-foreground underline underline-offset-4");

/// The DOM id of one piece of a table's chrome, distinct per table on the page.
pub(super) fn table_dom_id(state: &TableState, suffix: &str) -> String {
    format!("{}-{suffix}", state.prefix.as_deref().unwrap_or("table"))
}

/// A table link's attributes: `href`, and a plain click that writes the link's query to the
/// table's `query` signal instead of navigating, so the page reruns with the new state in place.
/// A modified click opens `href` the browser's way.
pub(super) fn live_link(cx: &Cx, url: String, signals: &TableSignals) -> Attributes {
    let query = signals.query.clone();
    let next = query_of(&url).to_string();
    attributes! {
        cx =>
        href=(url)
        @click=$(|e: Event| {
            if e.ctrl_key {
                return;
            }
            if e.meta_key {
                return;
            }
            if e.shift_key {
                return;
            }
            if e.alt_key {
                return;
            }
            e.prevent_default();
            query.set(next.clone());
        })
    }
}
