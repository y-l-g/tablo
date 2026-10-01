//! Live-search registry + shard dispatch.
//!
//! `#[shard]` inventory only discovers concrete fns, so each declared
//! resource monomorphizes its table loader here, keyed by list path.

use std::{future::Future, pin::Pin, sync::Arc};

use http_body_util::BodyExt;
use toasty::stmt::IntoExpr;
use topcoat::{
    Result,
    context::Cx,
    router::{
        Body, Layer, LayerFuture, Next, Path, PathBuf,
        error::{content_too_large, forbidden},
    },
    runtime::shard,
    view::{BoxView, View},
};

use super::{
    build::route_path,
    gate::{enforce_auth, gate, panel_prefix},
    list::{load_table_page, table_error_view, wire_table_actions},
    state::{CurrentPanel, current, panels},
};
use crate::{
    form::FormScalar,
    resource::{Resource, TableSignals, TableState},
    schema::FieldLens,
};

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

/// A relation table's live-search request: the owner's key in form spelling
/// (`seed`), the record page the table renders on (`page`, the links' target
/// and the writes' return), and whether that page shows rows without write
/// actions (`read_only`, the detail page).
pub(crate) struct RelationRequest {
    pub(crate) seed: String,
    pub(crate) page: String,
    pub(crate) read_only: bool,
}

/// A monomorphized live-search relation loader, one per declared relation.
///
/// Built by [`Relation::has_many`](crate::resource::Relation::has_many), which
/// is the one site that names both the child resource and the typed foreign
/// key, and keyed by `(parent slug, child slug)`. The seed travels as a string
/// because shard arguments cross the browser; the handler parses it back
/// through the key's [`FormScalar`] spelling, so a tampered seed that does not
/// parse is refused rather than loaded.
pub(crate) type RelationSearchFn = Arc<
    dyn for<'a> Fn(
            &'a Cx,
            RelationRequest,
            TableSignals,
        ) -> Pin<Box<dyn Future<Output = Result<BoxView<'a>>> + Send + 'a>>
        + Send
        + Sync,
>;

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

/// Monomorphize `C`'s relation loader over the typed foreign key into a
/// [`RelationSearchFn`]: tenancy + policy gate, then the same scoped load +
/// live render the streamed relation uses.
///
/// `read_only` selects the row chrome, exactly as the page does: a tampered
/// flag only changes which links render, never what the routes answer — every
/// write route re-checks its own policy. `page` must sit under the panel
/// prefix, like every page the panel serves; anything else is refused rather
/// than reflected into the table's links. The scope narrows the child rows to
/// the owner, so the output is a subset of what `C`'s list serves the same
/// caller — a tampered seed discloses no row the list hides.
pub(crate) fn relation_search_handler_for<C: Resource, T>(
    foreign_key: FieldLens<C::Model, T>,
) -> RelationSearchFn
where
    T: IntoExpr<T> + FormScalar + Send + Sync + 'static,
{
    Arc::new(
        move |cx: &Cx,
              ctx: RelationRequest,
              signals: TableSignals|
              -> Pin<Box<dyn Future<Output = Result<BoxView<'_>>> + Send + '_>> {
            let foreign_key = foreign_key.clone();
            Box::pin(async move {
                gate::<C>(cx)?;
                if !C::can_view_any(cx) {
                    return Err(forbidden().into());
                }
                let prefix = panel_prefix(cx);
                if ctx
                    .page
                    .strip_prefix(prefix.as_str())
                    .is_none_or(|rest| !rest.is_empty() && !rest.starts_with(['/', '?']))
                {
                    return Err(topcoat::router::error::bad_request("unknown relation page").into());
                }
                let owner = T::parse_form(ctx.seed.trim())
                    .map_err(|_| topcoat::router::error::bad_request("unknown relation owner"))?;
                let scope = foreign_key.eq(owner);
                let chrome = super::relations::relation_chrome::<C>(cx, ctx.read_only);
                let table = super::list::wire_table::<C>(cx, true, chrome);
                // The GET path's prefixed parser over the client-owned query,
                // then one normalization: an unknown `group_by` must not echo
                // through the retry link. The page renders the delete dialog
                // outside the swapped region, so the shard drops it.
                let mut state = TableState::from_query_prefixed(&signals.query.get(), &C::slug());
                state.delete = None;
                state.open = None;
                let state = table.normalize_state(&state);
                let table = table.returning_to(state.list_url(&ctx.page));
                // The retry link inside a failed table writes the same query
                // signal the controls do, so keep a handle for it.
                let retry_signals = signals.clone();
                let rendered = async {
                    let rows =
                        super::list::load_scoped_page::<C>(cx, &table, &state, scope).await?;
                    table
                        .render_live(cx, rows, &state, &ctx.page, signals)
                        .await
                };
                match rendered.await {
                    Ok(view) => Ok(view),
                    Err(error) => Ok(table_error_view::<C>(
                        cx,
                        &state,
                        &error,
                        &ctx.page,
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
    current(cx)
        .and_then(|panel| panel.search.get(path).cloned())
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
pub(crate) const TABLE_SEARCH_PATH: &str = "/_topcoat/runtime/shards/tablo-table-search";

/// Resolve the registered live-search relation handler for the
/// (`parent`, `child`) pair, answering the gate first (defense in depth):
/// the registry lookup runs only for an authenticated request, so an unknown
/// pair cannot be distinguished from a registered one by an unauthenticated
/// probe (404-vs-401 oracle).
fn relation_entry(cx: &Cx, parent: &str, child: &str) -> Result<RelationSearchFn> {
    enforce_auth(cx)?;
    current(cx)
        .and_then(|panel| {
            panel
                .relations
                .get(&(parent.to_string(), child.to_string()))
                .cloned()
        })
        .ok_or_else(|| topcoat::router::error::not_found().into())
}

/// Live relation-table interactions: re-renders one record page's relation
/// table as its signals change, morphing in place per Topcoat #392 (focus,
/// scroll and typing survive; rows carry stable `id`s).
///
/// The shard owns no state: the page creates the signals ([`TableSignals`])
/// keyed by page and relation key, renders the search and filter bars against
/// them above the swapped region, and passes their handles here. Search, sort,
/// filters and pagination all write the `query` signal, so one dependency
/// re-renders the table without a navigation or a scroll jump.
///
/// Every arg is untrusted shard input: `scope` must name a registered
/// (`parent`, `child`) pair and the owner's seed as `{parent}/{child}/{seed}`,
/// `page` must sit under the panel prefix, and the query is parsed by
/// [`TableState::from_query_prefixed`] with the GET path's bounds.
/// Authorization mirrors the relation table (`requires_tenant` +
/// `can_view_any`, row scoping via the tenant-scoped query plus the owner's
/// scope); shard POSTs carry no CSRF token, and none is needed for this
/// read-only rerun.
#[shard("/_topcoat/runtime/shards/tablo-table-relation-search")]
pub(crate) async fn table_relation_search(
    cx: &Cx,
    scope: String,
    page: String,
    read_only: bool,
    query: topcoat::runtime::Signal<String>,
    bulk: topcoat::runtime::Signal<String>,
) -> Result<impl View> {
    // Slugs never carry `/` (the panel refuses them at registration), so the
    // pair splits off the front and the seed keeps the rest, slashes
    // included; anything else misses the registry as 404.
    let mut parts = scope.splitn(3, '/');
    let (parent, child, seed) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    let entry = relation_entry(cx, parent, child)?;
    entry(
        cx,
        RelationRequest {
            seed: seed.to_string(),
            page,
            read_only,
        },
        TableSignals { query, bulk },
    )
    .await
}

/// The endpoint [`table_relation_search`] is served at.
///
/// The same stability and gate coverage contract as
/// [`TABLE_SEARCH_PATH`](self::TABLE_SEARCH_PATH): the literal in
/// [`table_relation_search`]'s attribute is the same path;
/// `table_relation_search_endpoint_is_the_named_path` pins the two together.
pub(crate) const TABLE_RELATION_SEARCH_PATH: &str =
    "/_topcoat/runtime/shards/tablo-table-relation-search";

/// The largest shard request [`ShardPanel`] reads: arguments and signal
/// values, a list path and a query string each, far below it.
const MAX_SHARD_BYTES: usize = 64 * 1024;

/// Puts the panel a live table's shard re-renders for on the request.
///
/// A shard is served at one runtime path for every panel, so no panel's
/// prefix layer runs for its re-render. The page names the panel in the
/// shard's arguments — the list path [`table_search`] takes, the record page
/// [`table_relation_search`] takes — and this layer reads that argument from
/// the body Topcoat's runtime posts (`{"args": [..], ..}`), finds the panel
/// serving it, and hands the request on with that panel and the same bytes.
/// An argument no panel serves leaves the request as it came, and the shard's
/// registry lookup answers 404.
pub(crate) struct ShardPanel {
    path: PathBuf,
    /// The index of the argument holding a path under the panel's prefix.
    arg: usize,
}

impl ShardPanel {
    pub(crate) fn new(path: &str, arg: usize) -> Self {
        Self {
            path: route_path(path),
            arg,
        }
    }
}

/// The part of a shard request [`ShardPanel`] reads.
#[derive(serde::Deserialize)]
struct ShardArgs {
    args: Vec<serde_json::Value>,
}

impl Layer for ShardPanel {
    fn path(&self) -> Option<&Path> {
        Some(&self.path)
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let bytes = http_body_util::Limited::new(body, MAX_SHARD_BYTES)
                .collect()
                .await
                .map_err(|_| content_too_large())?
                .to_bytes();
            let panel = serde_json::from_slice::<ShardArgs>(&bytes)
                .ok()
                .and_then(|request| request.args.get(self.arg)?.as_str().map(str::to_owned))
                .and_then(|path: String| panels(cx)?.by_path(&path).cloned());
            let body = Body::from(bytes);
            match panel {
                Some(panel) => next.run(&cx.with(CurrentPanel(panel)), body).await,
                None => next.run(cx, body).await,
            }
        })
    }
}

#[cfg(test)]
mod tests;
