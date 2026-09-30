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
use crate::resource::{Resource, TableSignals};

/// One live-table shard invocation: the list path the page asks for
/// and the interaction signals it owns.
///
/// The one argument the shard's *handler* takes. The `#[shard]` entry packs
/// its wire parameters into this, and every seam below — the registry lookup,
/// the state rebuild, the load, the render, and the retry link — reads the
/// same value, so a new interaction dimension never changes a signature
/// here.
pub(crate) struct TableSearchArgs {
    /// The list path to rerun: resolved through the registry (an allow-list,
    /// never a raw route).
    pub(crate) path: String,
    /// The page's signals, untrusted by the time the shard reads them back.
    pub(crate) signals: TableSignals,
}

/// A monomorphized live-search table loader, one per declared resource.
///
/// `#[shard]` inventory only discovers concrete fns, so the single
/// concrete [`table_search`] shard dispatches through this registry instead
/// of going generic. Built by [`Panel::resource`], keyed by list path.
pub(crate) type SearchFn = Arc<
    dyn for<'a> Fn(
            &'a Cx,
            TableSearchArgs,
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
/// The table catches its own load errors: a tampered `after=` /
/// `before=` signal fails to decode inside the shard invocation, and the
/// invocation must render the branded in-region `ErrorState` + retry link
/// (via `super::list::table_error_view`, same as the streamed list) instead of
/// erroring the shard. Auth/tenancy/policy failures still propagate — they
/// are not table evidence.
pub(crate) fn search_handler_for<R: Resource>() -> SearchFn {
    Arc::new(
        |cx: &Cx,
         args: TableSearchArgs|
         -> Pin<Box<dyn Future<Output = Result<BoxView<'_>>> + Send + '_>> {
            Box::pin(async move {
                gate::<R>(cx)?;
                if !R::can_view_any(cx) {
                    return Err(forbidden().into());
                }
                let table = wire_table_actions::<R>(cx, true);
                let TableSearchArgs { path, signals } = args;
                // One shared bound and one normalization per request (GH #148):
                // `TableSignals::to_state` applies the same `q` clamp and
                // `filters` bound the GET path applies. The shard `group_by` arg
                // is client input — an
                // unknown value must not echo through the retry link.
                // The render below takes the proof and does not
                // normalize again.
                let state = table.normalize_state(&signals.to_state());
                // The retry link inside a failed table writes the same signals
                // the toolbar does, so keep a handle for it.
                let retry_signals = signals.clone();
                let rendered = async {
                    let page = load_table_page::<R>(cx, &table, &state).await?;
                    table
                        .render_live_normalized(cx, page, &state, &path, signals)
                        .await
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
/// renders the toolbar against them, and passes their handles here. Search,
/// sort, filters and pagination all write those signals, so one dependency
/// graph re-renders the table without a navigation or a scroll jump.
///
/// Every arg is untrusted shard input: `path` must name a registered list, and
/// every signal value is clamped or re-parsed through
/// [`TableSignals::to_state`] like the GET path. Authorization mirrors the list
/// page (`requires_tenant` + `can_view_any`, row scoping via the tenant-scoped
/// query); shard POSTs carry no CSRF token, and none is needed for this
/// read-only rerun.
///
/// The module exists only to carry `allow(too_many_arguments)`: the shard's
/// arity *is* the interaction list. The wire stays scalar by choice — a struct
/// cannot travel as a shard argument (topcoat requires the `expr!` vocabulary,
/// and a struct has no `Surrogated` surrogate the browser's `cx.hydrate` can
/// rebuild), and packing the dimensions into a list would couple the browser to
/// this server's ordering.
#[allow(clippy::too_many_arguments)]
mod shard_body {
    use super::*;

    #[shard("/_topcoat/runtime/shards/tablo-table-search")]
    pub(crate) async fn table_search(
        cx: &Cx,
        path: String,
        q: topcoat::runtime::Signal<String>,
        filters: topcoat::runtime::Signal<String>,
        sort: topcoat::runtime::Signal<String>,
        dir: topcoat::runtime::Signal<String>,
        cursor: topcoat::runtime::Signal<String>,
        group_by: topcoat::runtime::Signal<String>,
        bulk: topcoat::runtime::Signal<String>,
    ) -> Result<impl View> {
        let entry = search_entry(cx, &path)?;
        // One argument struct from here down: the handler owns the
        // wire arity, nothing below it does.
        entry(
            cx,
            TableSearchArgs {
                path,
                signals: TableSignals {
                    q,
                    filters,
                    sort,
                    dir,
                    cursor,
                    group_by,
                    bulk,
                },
            },
        )
        .await
    }
}
pub(crate) use shard_body::table_search;

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
