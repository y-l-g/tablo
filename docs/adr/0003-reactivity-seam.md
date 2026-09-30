# Reactivity: suspense, morphing reruns, and the live-search shard

Date: 2026-08-19 — Status: accepted — Amended: 2026-09-10, 2026-09-22, 2026-09-30

## Decision

Tablo's reactivity is committed behind owned APIs: page state (query/sort/page) is owned by the
page, and resources never write their own `#[shard]` — a shard endpoint stays an optimization, not
the API surface. The migration to the current Topcoat runtime is done: `suspense` streams the
resource list behind a skeleton of the table's own header, and later reruns (page or shard) morph in
place (topcoat #392) so focus, scroll, and typing survive; reorderable rows need stable `id`s.
Shards take `Signal<T>` params (#393), and the keystroke-live, slug-dispatched `table_search` shard
sits behind `Table::live_search`, written by search, sort, filters, and pagination. The table always
renders inside a `data-boundary` region.

The `table_search` shard takes two signals besides `path`: `query`, the list's URL query, and
`bulk`, the selection. The URL query is the one spelling of list state: every control already
renders its target URL in its `href`, so a live control writes that URL's query, and the search and
filter scripts edit their own keys of the current value. The shard parses it with
`TableState::from_query`, the GET page's parser, so the live table and the page cannot disagree
about what a query means, and a new state dimension changes no shard signature. Two string signals
need no struct-typed shard argument (#337). The selection is not URL state, so it keeps its own
signal and survives a rerun. `TableState::to_signals` seeds both from the parsed state, and a
request normalizes the parsed state once, through `Table::normalize_state`.