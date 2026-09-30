# Shell JS assets: ownership, all-load policy, and the hook contract

Date: 2026-09-18 — Status: accepted — Amended: 2026-09-19, 2026-09-22, 2026-09-23, 2026-09-25, 2026-09-27, 2026-09-30

## Decision

**Ownership.** `crates/tablo-ui/assets/` holds eleven hand-written JS assets (`sidebar.js`,
`theme.js`, `dialog.js`, `wire.js`, `bulk.js`, `filters.js`, `live-search.js`, `selects.js`, `variant.js`,
`notifications.js`, `mutation-submit.js`; `selects.test.js`, `bulk.test.js`, `wire.test.js`, `dialog.test.js`,
`mutation-submit.test.js`, `notifications.test.js`, `filters.test.js`, and `live-search.test.js` are
the Node tests, not
shipped, and `examples/showcase/assets/media.test.js` tests the showcase's `media.js` — ~71.8 KB
unminified, ~27.4 KB gzipped summed per asset (`gzip -9 -n`), with no build or minify step). They
are declared as `Asset` constants in
`crates/tablo-ui/src/lib.rs` and emitted by `Panel::render_document` in `tablo-core` on every
document with `ShellAssets`, including the login page, where all but `theme.js`'s backstop apply are
no-ops. Only the document emits `<script>` tags, `defer`red (GH #152 — parsing never waits for them;
every asset either registers document-level listeners at execution or binds in a `DOMContentLoaded`
handler). The blocking `theme_init_script` stays inline so the `dark` class applies before first
paint.
Per-component `<script>` tags stay out: duplicate execution stacks document listeners, and the runtime
does not manage script lifecycles in swapped content.

**All-load policy.** Every document with `ShellAssets` loads all eleven scripts. `render_document`
receives an opaque `BoxView` and `layout_shell` a lazy `Slot`, so nothing at document level can
observe what was rendered; scoping emission to page content needs a new declaration API, and it would
not shrink the bundle because all eleven handles stay referenced. The "hook ⇒ script" guarantee
therefore holds only for documents rendered through `render_document` with `ShellAssets` configured:
a `Panel` built without `.shell_assets(..)` renders sidebar/toaster hooks with no scripts, as do apps
using `tablo-ui` components directly. Each asset is served from a content-hashed URL with an
immutable `Cache-Control`, so the set costs one cold fetch per browser per build rather than a fetch
per document: the first page a visitor loads carries it and the rest read the cache. Every
document still parses and runs all eleven, so the cache removes the fetch, not the work. A set
decided from the panel's own declarations needs no new view input, but it would carry the panel's
set onto the login page, and a per-page declaration would rest on a guarantee
`verify_asset_hooks` cannot see, because it greps sources rather than rendered documents
(GH #312).

**Hook contract.** Each asset consumes an explicit hook list, guarded by `xtask/tests/it.rs`
(via `xtask::verify_asset_hooks`, alongside the registry-sync guard): the test fails when an asset file is
missing or renamed, or when a listed hook no longer appears in both its JS asset and the Rust sources.
The list is attribute hooks only — structural selectors (`.relative`, `pre code`, `select option`,
`dialog[open]`) and the inverse direction (a rendered hook with no consumer) are out of scope.

| Asset | Hooks (Rust render site → JS consumer) | Without the script |
|---|---|---|
| `sidebar.js` | `data-sidebar`, `data-state` (sidebar primitive), `sidebar_state` cookie (shell) | State no longer persists; `Ctrl+B` dies |
| `theme.js` | `data-theme-toggle` (shell) | Toggle inert; init script still paints the stored theme |
| `dialog.js` | `data-dialog-close`, `data-dialog-open-param`, `data-row-delete-trigger`, `data-row-delete-action`, `data-row-delete-form` (row delete dialog) | Row Delete still opens the dialog through `?delete=` and Delete still POSTs; Cancel is inert, and Escape/backdrop do not dismiss |
| `wire.js` | No hooks — the shared `,a,b,` selection-wire codec `bulk.js` writes and `mutation-submit.js` prunes (GH #341) | Both consumers throw on load; bulk delete unusable |
| `bulk.js` | `data-bulk-form`, `data-table-root`, `data-bulk-confirm-trigger`, `data-bulk-confirm-dialog`, `data-bulk-confirm-description`, `data-row-select`, `data-bulk-select-all`, `ids` transport (table) | Bulk delete unusable |
| `filters.js` | `data-filter-name`, `data-filters-form`, `data-filters-transport`, `data-filters-live` (filter bar) | Typed controls inert; `<noscript>` free-text + Apply keeps working |
| `live-search.js` | `data-live-search`, `data-live-search-input`, `data-live-search-transport`, `data-debounce-ms` (live table toolbar) | Typing no longer debounces into a reload; the `<noscript>` GET form is the search path |
| `selects.js` | `data-select-filterable`, `data-options-filter` (searchable `Select`) | Filter input inert; plain select keeps working |
| `variant.js` | `data-variant-select`, `data-variant-of`, `data-variant` (embedded enum groups) | Every variant's group renders; nothing the server parses is lost |
| `notifications.js` | `data-sonner-toast`, `data-close-button`, `data-mounted` (toaster) | Toasts stay visible until the next navigation |
| `mutation-submit.js` | `data-mutation-submit` (row + bulk confirms), `data-table-revision` (live table), `data-boundary` (table region), `data-sonner-toaster` (shell) | Both confirms POST and 303; the table updates with a full page load |

**No-build stance.** No `package.json`, no lint/format config, no dependency install, no minification,
and no bundling step: the assets are small enough that a toolchain would cost more than it saves. CI
still runs them: the `assets` job names each suite and runs it with `node --test` on the runner's Node,
with nothing to install first. Revisit with the all-load policy if they grow.

**No-JS posture.** `sidebar.js` (mobile nav persistence) and `bulk.js` (bulk delete) are load-bearing
for their features; `theme.js` is needed for the toggle; the other eight are progressive enhancements
with fallbacks, as the table records. Delegation is deliberate throughout: Topcoat morphs swapped
content with no script-lifecycle handling, so document-level listeners (plus `notifications.js`'s
`MutationObserver` for mounted-state arming) keep behavior alive after post-load shard swaps.

**Where the requirement is documented.** Composites carry it in rustdoc (`toaster`); `dialog`/`sheet`
are vendored primitives, so the note lives at the core render sites (the delete dialog, the
`render_shell` mobile sheet) and on searchable `Select` in `schema.rs` (GH #152).

## Consequences

`Panel::render_document` keeps emitting all eleven tags, `defer`red; a new hook extends `ASSET_HOOKS`
with both sides in the same commit. `cargo xtask` still never touches `assets/` (ADR-0007 covers
primitives only).
