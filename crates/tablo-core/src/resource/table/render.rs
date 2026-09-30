//! [`Table`](super::Table) HTML rendering: the entry points plus the chrome.
//!
//! Grouping is page-local and interleaved and a page encodes its row-action
//! URL base once.
//!
//! The entry points and the table chrome live in `core`, the header row in
//! `head`, row projection and rendering in `rows`, the zero-rows cell in
//! `empty`, column widths in `widths`, the search and bulk bars in `toolbar`,
//! the filter bar in `filterbar`, pagination in `pager`, the delete dialog in
//! `dialog`, and the streamed placeholder in `skeleton`.

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

use super::super::state::{TableSignals, query_of};

/// A toolbar row above the table: the search-and-bulk row, the filter bar,
/// and the live search host.
const BAR_CLASS: StaticClass =
    class!("flex flex-wrap items-center gap-2 border-b border-border p-3");

/// The search form inside the toolbar row, beside the bulk form.
const SEARCH_FORM_CLASS: StaticClass = class!("flex flex-wrap items-center gap-2");

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

/// A table inside a card the page draws.
const TABLE_BARE_CLASS: StaticClass = class!("overflow-hidden");

/// A secondary link inside a bar ("Clear filters", "Clear search").
const QUIET_LINK_CLASS: StaticClass = class!("text-sm text-muted-foreground hover:text-foreground");

/// A link in the empty-table message ("Clear search", "Back to first page").
const EMPTY_LINK_CLASS: StaticClass =
    class!("text-sm font-medium text-foreground underline-offset-4 hover:underline");

/// A table link's attributes: `href` always, and on a live table (`signals`) a
/// click that writes the link's own query to the `query` signal instead of
/// navigating, so the shard re-renders the table in place. `href` stays the
/// no-JS fallback and spells the same state.
pub(super) fn live_link(cx: &Cx, url: String, signals: Option<&TableSignals>) -> Attributes {
    match signals {
        Some(signals) => {
            let query = signals.query.clone();
            let next = query_of(&url).to_string();
            attributes! {
                cx =>
                href=(url)
                @click=$(|e: Event| {
                    e.prevent_default();
                    query.set(next.clone());
                })
            }
        }
        None => attributes! { cx => href=(url) },
    }
}
