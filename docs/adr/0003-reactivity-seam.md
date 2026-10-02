# Reactivity: suspense, morphing reruns, and the live-search shard

Date: 2026-08-19 — Status: accepted

## Decision

Page state (query/sort/page) belongs to the page; resources declare no `#[shard]`. `suspense`
streams the list behind a table-header skeleton; reruns morph in place, preserving focus, scroll,
and typing. Reorderable rows need stable `id`s. Shards take `Signal<T>` params. `Table::live_search`
enables the keystroke-live `table_search` shard. The table renders inside a `data-boundary` region.

`table_search` takes `query` and `bulk` signals plus `path`. Controls render target URLs in
`href`; live controls edit the current query value. The shard parses with `TableState::from_query`,
the GET page parser, so both agree. `TableState::signals_for` seeds the query; each request
normalizes once through `Table::normalize_state`. Selection keeps its own signal and survives
reruns.
