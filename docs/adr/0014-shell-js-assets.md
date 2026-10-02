# Shell JS assets: ownership, all-load policy, and the hook contract

Date: 2026-09-18 — Status: accepted

## Decision

**Ownership.** `crates/tablo-ui/assets/` holds eleven hand-written JS assets (`sidebar.js`,
`theme.js`, `dialog.js`, `wire.js`, `bulk.js`, `filters.js`, `live-search.js`, `selects.js`,
`variant.js`, `notifications.js`, `mutation-submit.js`). They are `Asset` constants in
`crates/tablo-ui/src/lib.rs`, emitted by `Panel::render_document` on every document with
`ShellAssets`, including login. Only the document emits `<script>` tags, `defer`red; every asset
registers document-level listeners or binds on `DOMContentLoaded`. The blocking
`theme_init_script` stays inline and applies `dark` before first paint.

**All-load policy.** Every document with `ShellAssets` loads all eleven scripts. `render_document`
takes an opaque `BoxView` and `layout_shell` a lazy `Slot`, so document level observes no page
content. Each asset serves from a content-hashed URL with immutable caching, so the set costs one
cold fetch per browser per build. A panel without `.shell_assets(..)` renders hooks with no
scripts, as do direct `tablo-ui` users.

**Hook contract.** Each asset consumes an explicit hook list guarded by `xtask` tests: the test
fails when an asset is missing or a listed hook leaves JS or Rust sources. The list covers
attribute hooks only.

| Asset | Hooks (Rust render site → JS consumer) | Without the script |
|---|---|---|
| `sidebar.js` | `data-sidebar`, `data-state`, `sidebar_state` cookie | State no longer persists; `Ctrl+B` dies |
| `theme.js` | `data-theme-toggle` | Toggle inert; init script still paints stored theme |
| `dialog.js` | `data-dialog-close`, `data-dialog-open-param`, `data-row-delete-*` | Delete still POSTs; Cancel, Escape, backdrop inert |
| `wire.js` | Shared selection-wire codec | Both consumers throw; bulk delete unusable |
| `bulk.js` | `data-bulk-form`, `data-table-root`, `data-bulk-confirm-*`, `data-row-select`,
`data-bulk-select-all`, `ids` transport | Bulk delete unusable |
| `filters.js` | `data-filter-name`, `data-filters-form`, `data-filters-transport`,
`data-filters-live` | Typed controls inert; `<noscript>` form works |
| `live-search.js` | `data-live-search`, `data-live-search-input`, `data-live-search-transport`,
`data-debounce-ms` | Typing never reloads; `<noscript>` GET form works |
| `selects.js` | `data-select-filterable`, `data-options-filter` | Filter inert; plain select works |
| `variant.js` | `data-variant-select`, `data-variant-of`, `data-variant` | Every group renders; parsed values unchanged |
| `notifications.js` | `data-sonner-toast`, `data-close-button`, `data-mounted` | Toasts persist until navigation |
| `mutation-submit.js` | `data-mutation-submit`, `data-table-revision`, `data-boundary`,
`data-sonner-toaster` | Confirms POST and 303 with full page load |

**No-build stance.** No `package.json`, lint/format config, install, minification, or bundling.
CI runs each suite with `node --test` and nothing to install.

**No-JS posture.** `sidebar.js` and `bulk.js` are load-bearing for their features; `theme.js` for
the toggle; the other eight are progressive enhancements with fallbacks. Delegation keeps behavior
alive after shard swaps: document-level listeners plus a `MutationObserver` for toast arming.

Composites carry the requirement in rustdoc (`toaster`); `dialog`/`sheet` notes live at core
render sites and searchable `Select` in `schema.rs`.
