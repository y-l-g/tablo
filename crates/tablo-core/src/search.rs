//! The results under the global search box a panel shows in its topbar, as
//! [`Panel::without_global_search`](crate::Panel::without_global_search) describes it.
//!
//! The results are a shard: once the page loads, each pause in typing renders them again for the
//! new term. Without JavaScript the box's form loads `{prefix}/-/search?q=` instead.

use topcoat::{
    Result,
    context::Cx,
    runtime::shard,
    view::{View, ViewExt},
};

/// The first results of every resource the panel at `prefix` searches for `term`, and a link to
/// the results page; nothing for a blank term.
///
/// A named path stays stable across builds (topcoat#441), and staying under `/_topcoat/runtime`
/// keeps the panel's auth gate covering it.
#[shard("/_topcoat/runtime/shards/tablo-search")]
pub async fn search_results(cx: &Cx, prefix: String, term: String) -> Result<impl View> {
    Ok(crate::panel::render_search_results(cx, prefix, term)
        .await?
        .boxed())
}
