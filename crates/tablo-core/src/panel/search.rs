//! Live-search registry + shard dispatch.
//!
//! `#[shard]` inventory only discovers concrete fns, so each declared
//! resource monomorphizes its table loader here, keyed by list path.

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use topcoat::{
    Result,
    context::Cx,
    router::error::forbidden,
    runtime::shard,
    view::{BoxView, View},
};

use super::{
    gate::{enforce_auth, gate},
    list::{load_table_page, table_error_view, wire_table_actions},
};
use crate::resource::{Resource, TableSignals, TableState};

/// A monomorphized live-search table loader, one per declared resource.
///
/// `#[shard]` inventory only discovers concrete fns, so the single
/// concrete [`table_search`] shard dispatches through this registry instead
/// of going generic. Built by [`Panel::resource`], keyed by list path.
pub(crate) type SearchFn = Arc<
    dyn for<'a> Fn(
            &'a Cx,
            String,
            TableSignals,
        ) -> Pin<Box<dyn Future<Output = Result<BoxView<'a>>> + Send + 'a>>
        + Send
        + Sync,
>;

/// Live-search handlers installed on the app context by [`Panel::build`].
#[derive(Clone, Default)]
pub(crate) struct SearchRegistry(pub(crate) HashMap<String, SearchFn>);

/// Monomorphize `R`'s table loader into a [`SearchFn`]: tenancy + policy gate,
/// then the same load + render the streamed list uses.
///
/// The table catches its own load errors: a tampered cursor in the query
/// fails to decode inside the shard invocation, and the invocation must render
/// the branded in-region `ErrorState` + retry link (via
/// `super::list::table_error_view`, same as the streamed list) instead of
/// erroring the shard. Auth/tenancy/policy failures still propagate — they
/// are not table evidence.
pub(crate) fn search_handler_for<R: Resource>() -> SearchFn {
    Arc::new(
        |cx: &Cx,
         path: String,
         signals: TableSignals|
         -> Pin<Box<dyn Future<Output = Result<BoxView<'_>>> + Send + '_>> {
            Box::pin(async move {
                gate::<R>(cx)?;
                if !R::can_view_any(cx) {
                    return Err(forbidden().into());
                }
                let table = wire_table_actions::<R>(cx, true);
                // The GET path's parser over the client-owned query, then one
                // normalization (GH #148): an unknown `group_by` must not echo
                // through the retry link. The live page renders the delete
                // dialog outside the swapped region, so the shard drops it.
                let mut state = TableState::from_query(&signals.query.get());
                state.delete = None;
                state.open = None;
                let state = table.normalize_state(&state);
                // The retry link inside a failed table writes the same query
                // signal the controls do, so keep a handle for it.
                let retry_signals = signals.clone();
                let rendered = async {
                    let page = load_table_page::<R>(cx, &table, &state).await?;
                    table.render_live(cx, page, &state, &path, signals).await
                };
                match rendered.await {
                    Ok(view) => Ok(view),
                    Err(error) => Ok(table_error_view::<R>(
                        cx,
                        &state,
                        &error,
                        &path,
                        Some(&retry_signals),
                    )),
                }
            })
        },
    )
}

/// Resolve the registered live-search handler for `path`, answering the gate
/// first (defense in depth): the registry lookup runs only for an
/// authenticated request, so an unknown `path` cannot be distinguished from a
/// registered one by an unauthenticated probe (404-vs-401 oracle).
fn search_entry(cx: &Cx, path: &str) -> Result<SearchFn> {
    enforce_auth(cx)?;
    topcoat::context::try_app_context::<SearchRegistry>(cx)
        .and_then(|reg| reg.0.get(path).cloned())
        .ok_or_else(|| topcoat::router::error::not_found().into())
}

/// Live table interactions: re-renders one resource's table as its signals
/// change, morphing in place per Topcoat #392 (focus, scroll and typing
/// survive; rows carry stable `id`s).
///
/// The shard owns no state: the page creates the signals ([`TableSignals`]),
/// renders its controls against them, and passes their handles here. Search,
/// sort, filters and pagination all write the `query` signal, so one
/// dependency re-renders the table without a navigation or a scroll jump.
///
/// Every arg is untrusted shard input: `path` must name a registered list, and
/// the query is parsed by [`TableState::from_query`] with the GET path's
/// bounds. Authorization mirrors the list page (`requires_tenant` +
/// `can_view_any`, row scoping via the tenant-scoped query); shard POSTs carry
/// no CSRF token, and none is needed for this read-only rerun.
#[shard("/_topcoat/runtime/shards/tablo-table-search")]
pub(crate) async fn table_search(
    cx: &Cx,
    path: String,
    query: topcoat::runtime::Signal<String>,
    bulk: topcoat::runtime::Signal<String>,
) -> Result<impl View> {
    let entry = search_entry(cx, &path)?;
    entry(cx, path, TableSignals { query, bulk }).await
}

/// The endpoint [`table_search`] is served at.
///
/// A shard that declares no path is served at a build-random one (topcoat#441);
/// naming it keeps the endpoint stable across builds and legible in logs and
/// tests. The `/tablo-` prefix separates it from a generated path, and the
/// whole path stays under `/_topcoat/runtime`, so the panel's auth gate covers
/// the shard and an unauthenticated rerun answers 401 rather than redirecting to
/// the login page.
///
/// The literal in [`table_search`]'s attribute is the same path;
/// `table_search_endpoint_is_the_named_path` pins the two together.
#[cfg(test)]
pub(crate) const TABLE_SEARCH_PATH: &str = "/_topcoat/runtime/shards/tablo-table-search";

#[cfg(test)]
mod tests;
