//! Live-search registry and shard dispatch keyed by list path.

use std::{future::Future, pin::Pin, sync::Arc};

use http_body_util::BodyExt;
use topcoat::{
    Result,
    context::Cx,
    router::{
        Body, Layer, LayerFuture, Next, Path, PathBuf,
        error::{content_too_large, forbidden},
    },
    runtime::shard,
    view::{BoxView, View, view},
};

use super::{
    build::route_path,
    gate::gate,
    list::{load_table_page, table_error_view, wire_table_actions},
    state::{CurrentPanel, current, panels},
};
use crate::{
    policy::Ability,
    resource::Resource,
    table::{TableSignals, TableState},
};

/// A resource's live-search table loader, keyed by list path.
pub(crate) type SearchFn =
    for<'a> fn(
        &'a Cx,
        String,
        TableSignals,
    ) -> Pin<Box<dyn Future<Output = Result<BoxView<'a>>> + Send + 'a>>;

/// A relation table's live-search request: the owner's key in form spelling
/// (`seed`), the record page the table renders on (`page`, the links' target
/// and the writes' return), and whether that page shows rows without write
/// actions (`read_only`, the detail page).
pub(crate) struct RelationRequest {
    pub(crate) seed: String,
    pub(crate) page: String,
    pub(crate) read_only: bool,
}

/// Monomorphizes a relation's live-search loader keyed by (parent slug, child slug).
pub(crate) type RelationSearchFn = Arc<
    dyn for<'a> Fn(
            &'a Cx,
            RelationRequest,
            TableSignals,
        ) -> Pin<Box<dyn Future<Output = Result<BoxView<'a>>> + Send + 'a>>
        + Send
        + Sync,
>;

/// Serves `R`'s live list table: the [`SearchFn`] its list registers.
pub(crate) fn list_search<R: Resource>(
    cx: &Cx,
    path: String,
    signals: TableSignals,
) -> Pin<Box<dyn Future<Output = Result<BoxView<'_>>> + Send + '_>> {
    Box::pin(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can(cx, Ability::ViewAny) {
            return Err(forbidden().into());
        }
        let table = wire_table_actions(cx, &resource, true);
        let mut state = TableState::from_query(&signals.query.get());
        state.delete = None;
        state.open = None;
        let state = table.normalize_state(&state);
        let retry_signals = signals.clone();
        let rendered = async {
            let page = load_table_page(cx, &resource, &table, &state).await?;
            table.render_live(cx, page, &state, &path, signals).await
        };
        match rendered.await {
            Ok(view) => Ok(view),
            Err(error) => Ok(table_error_view(
                cx,
                &resource,
                &state,
                &error,
                &path,
                Some(&retry_signals),
            )),
        }
    })
}

/// Resolves the live-search handler for `path`, answering the gate before the registry lookup.
fn search_entry(cx: &Cx, path: &str) -> Result<SearchFn> {
    crate::auth::guard(cx)?;
    current(cx)
        .and_then(|panel| panel.search.get(path).copied())
        .ok_or_else(|| topcoat::router::error::not_found().into())
}

/// Re-renders one resource's table as its signals change (Topcoat #392).
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

/// The `table_search` shard invocation that fills a live list's streamed region at `path`.
pub(crate) fn list_search_invocation<'a>(
    cx: &'a Cx,
    path: &str,
    signals: TableSignals,
) -> impl View + use<'a> {
    let path = path.to_string();
    let TableSignals { query, bulk } = signals;
    view! { cx => table_search(path: $(path.clone()), query: $(query), bulk: $(bulk)) }
}

/// Names the [`table_search`] endpoint (topcoat#441) under `/_topcoat/runtime` so the panel's auth
/// gate covers it.
pub(crate) const TABLE_SEARCH_PATH: &str = "/_topcoat/runtime/shards/tablo-table-search";

/// Resolves the relation handler for (`parent`, `child`), answering the gate before the registry
/// lookup.
fn relation_entry(cx: &Cx, parent: &str, child: &str) -> Result<RelationSearchFn> {
    crate::auth::guard(cx)?;
    current(cx)
        .and_then(|panel| {
            panel
                .relations
                .get(&(parent.to_string(), child.to_string()))
                .cloned()
        })
        .ok_or_else(|| topcoat::router::error::not_found().into())
}

/// Re-renders one record page's relation table as its signals change (Topcoat #392).
#[shard("/_topcoat/runtime/shards/tablo-table-relation-search")]
pub(crate) async fn table_relation_search(
    cx: &Cx,
    scope: String,
    page: String,
    read_only: bool,
    query: topcoat::runtime::Signal<String>,
    bulk: topcoat::runtime::Signal<String>,
) -> Result<impl View> {
    // Slugs never carry `/`, so the pair splits off the front and the seed keeps the rest.
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

/// The `table_relation_search` shard invocation that fills a live relation table's streamed
/// region: `child`'s table under the `parent` record `request` names.
pub(crate) fn relation_search_invocation<'a>(
    cx: &'a Cx,
    parent: &str,
    child: &str,
    request: RelationRequest,
    signals: TableSignals,
) -> impl View + use<'a> {
    let RelationRequest {
        seed,
        page,
        read_only,
    } = request;
    // Slugs never carry `/`, so the pair and the seed travel as one wire arg, split off the front
    // by the shard.
    let scope = format!("{parent}/{child}/{seed}");
    let TableSignals { query, bulk } = signals;
    view! {
        cx =>
        table_relation_search(
            scope: $(scope.clone()),
            page: $(page.clone()),
            read_only: $(read_only),
            query: $(query),
            bulk: $(bulk)
        )
    }
}

/// Names the [`table_relation_search`] endpoint with the same stability and gate coverage as
/// [`TABLE_SEARCH_PATH`].
pub(crate) const TABLE_RELATION_SEARCH_PATH: &str =
    "/_topcoat/runtime/shards/tablo-table-relation-search";

/// Bounds the shard request [`ShardPanel`] reads.
const MAX_SHARD_BYTES: usize = 64 * 1024;

/// Puts the panel a live table's shard re-renders for on the request.
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
