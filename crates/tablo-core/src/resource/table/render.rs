//! [`Table`](super::Table) HTML rendering: the entry points plus the chrome.
//!
//! Grouping is page-local and interleaved and a page encodes the
//! filter transport once.
//!
//! The entry points and row assembly live in `core`, the search and bulk
//! bars in `toolbar`, the filter bar in `filterbar`, pagination in `pager`,
//! the delete dialog in `dialog`, and the streamed placeholder in `skeleton`.

mod core;
mod dialog;
mod filterbar;
mod pager;
mod skeleton;
mod toolbar;

use topcoat::{
    context::Cx,
    runtime::Event,
    view::{Attributes, attributes},
};

use super::super::state::{TableSignals, query_of};

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
