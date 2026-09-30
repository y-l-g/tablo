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

/// A toolbar bar above the table: the search bar, the filter bar, and the
/// live search host.
const BAR_CLASS: StaticClass =
    class!("flex flex-wrap items-center gap-2 border-b border-border p-3");

/// A secondary link inside a bar ("Clear filters", "Clear search").
const QUIET_LINK_CLASS: StaticClass = class!("text-sm text-muted-foreground hover:text-foreground");

/// A link in the empty-table message ("Clear search", "Back to first page").
const EMPTY_LINK_CLASS: StaticClass = class!("text-sm text-primary hover:underline");

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
