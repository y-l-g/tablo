# Architecture

How the crates fit together, what happens on a request, and where the extension points are.
`CONTEXT.md` defines the vocabulary; `docs/guide/` explains how to use the toolkit.

## Crates

`tablo-macros`, `tablo-ui` and `tablo-build` are leaves. `tablo-core` depends on `tablo-macros`
and `tablo-ui`, `tablo-test` on `tablo-core`, and the `tablo` facade on `tablo-core`, `tablo-ui` and (feature `testing`) `tablo-test`. An app names
`tablo` under `[dependencies]` and `tablo-build` under `[build-dependencies]`.

| Crate | Depends on | Contents |
| --- | --- | --- |
| `tablo` | `tablo-core`, `tablo-ui`, `tablo-test` (feature `testing`), `toasty` | the facade: `tablo_core` at its root, `ui`, `testing`, `prelude`, the driver features |
| `tablo-macros` | — | the `EmbeddedForm` and `RecordForm` derives |
| `tablo-ui` | `topcoat` | synced primitives, owned composites, `icons.rs` |
| `tablo-core` | `tablo-macros`, `tablo-ui`, `toasty` | Panel, Resource, Table, Schema, auth, tenancy, upload |
| `tablo-test` | `tablo-core`, `topcoat` | the in-memory HTTP client, `tablo::testing` |
| `tablo-build` | `topcoat` | `tailwind()`, the app's Tailwind build over Tablo's sources |
| `examples/showcase` | `tablo-core`, `tablo-ui`, `toasty`, `tablo-build` (build), `tablo-test` (dev) | the runnable admin and the integration tests |
| `examples/quickstart` | `tablo`, `tablo-build` | the smallest app, detached, built from outside the repo by `cargo xtask external-check` |

No library crate enables a database driver. Everything reaches the database through Toasty's `Db`
and `Executor`, and the app picks the driver: a `tablo` feature (`sqlite`, `postgresql`, `mysql`)
or a feature on its own `toasty` dependency. That is also why an app-level `Uploader` and the
`Authenticator` are traits the app implements rather than crates the toolkit picks.

The derives name `tablo_core` when the app depends on it, and `tablo` otherwise; the facade
re-exports everything the generated code reaches.

The panel's markup carries Tailwind classes in `tablo-core` and `tablo-ui`, so the app's stylesheet
must scan their sources wherever Cargo unpacked them. Each of the two declares a `links` key and
publishes its `src` directory as build-script metadata (`DEP_TABLO_CORE_SRC`, `DEP_TABLO_UI_SRC`);
`tablo` forwards them as `DEP_TABLO_CORE` and `DEP_TABLO_UI`, since a build script sees only its direct
dependencies' metadata. `tablo_build::tailwind()` reads them and adds one `@source` per directory
to the app's `styles.css`.

## The layering

```
Panel  ──declares──▶  Resource  ──declares──▶  Table   (the list view)
   │                      │                  └▶  Schema  (forms, detail pages)
   │                      └──record fns─────▶  create / update / delete
   └──owns──▶ routes, Shell, the auth gate (the app owns the Router and the Db)
```

A `Panel` is an admin panel under one prefix: its shell layout, its authentication gate, its
resources and pages. The app owns the router and the `Db` in its app context, and mounts the panel
with `RouterBuilderPanelExt::panel`; one router mounts several panels at distinct prefixes.
Registering a `Resource` or a `Page` on a panel adds its routes and its sidebar entry. A `Resource`
maps one Toasty model to its admin UI: a base query, a `Table`, a `Schema`, a policy, and the
record functions that perform writes.

Each mounted panel's state — navigation, brand, auth, uploader, the live-search registries — is
one `PanelState` in the router's `Panels`. The panel's gate layer puts it on every request under
its prefix, so a handler, the shell and the auth checks read the request's panel, never a
router-wide singleton. A live table's shard is served at one runtime path for every panel, so
`ShardPanel` reads the panel from the list path the shard is called with; the runtime gate resolves
a session through the auth of the panel that issued it.

`Table` and `Schema` are declarations, not renderers. Mounting the panel calls each resource's
`table()` (which takes no context), `form(dx)` / `view(dx)` (which take a `DeclCx` carrying
the app schema alone) and `relations()` once, checks those exact values, and stores them; every
handler serves the cached copy instead of rebuilding per request. Because the mount has no
request, a declaration must not need request-scoped context; one that cannot render fails the
mount rather than a request.

## A read request

A resource list page runs, in order:

1. `enforce_auth(cx)` — require the user the panel's gate resolved from the session, or redirect
   to the login page.
2. `enforce_tenant::<R>(cx)` — refuse with 403 when `R::requires_tenant()` and the request has no
   tenant.
3. `R::can_view_any(cx)` — the list-level policy check, before any row is loaded.
4. Parse `TableState` from the URL (`?q=`, `?sort=`, `?dir=`, `?after=`/`?before=`, `?f.<name>=`,
   `?group_by=`).
5. Load through `TablePage::load` over `scoped_query::<R>(cx)` — `R::query(cx)` with the
   framework's tenant filter ANDed on — which adds the relations the table's columns include.
6. Render the table inside a `suspense` region: the skeleton is sent with the shell, the loaded rows
   swap in.

The list checks `can_view_any` only, so pagination stays honest; per-row `can_view` trims the export
and the relationship option lists. A detail page loads through the tenant-scoped `view_query`, so an
unknown id and one outside the tenant are the same 404, while a row the caller may not view is a 403.
Each of its relations then loads the related resource's list through that resource's own scoped
query, narrowed to the record.

## A write request

Create, update, delete, and bulk delete run the same shape:

1. `enforce_auth`, `enforce_tenant`, and the policy check that needs no transaction: `can_create`
   for a create, `can_delete_any` for a delete or bulk delete, and `can_view` plus `can_update` on
   the stored record for an update.
2. For a form: on an edit, complete the keys the submission did not post from the stored record;
   then validate, which also resolves relationship fields against the related resource's query.
3. Open a framework-owned transaction, re-load the target through the scoped query, and check
   `can_view` plus `can_update` or `can_delete` on that row, so policy is checked against the row
   that is about to be written rather than the submitted id.
4. For a form: re-complete the unposted keys from that row, run the unique probe, parse the values
   into the resource's record form, and run `validate_record`; any error re-renders the form.
5. Call the resource's record function inside that transaction.
6. Commit, then call `Resource::after_commit(cx, committed)`.

Every POST carries a double-submit CSRF token, and a bulk delete additionally requires the
`confirm=1` marker that only the confirm control emits. The record functions and the custom
`Action`s a resource lists are the mutation vocabulary; an action runs through the same gate,
CSRF check, tenant-scoped load, per-record policy and transaction as a delete.

`after_commit` is the only place for a side effect that must not survive a rollback — email, a
webhook, an audit row. It runs after the transaction and before the response, it runs once per
committed write, and a failure in it is logged without rolling the write back.

## Extension points

| Seam | Where | What it decides |
| --- | --- | --- |
| `Resource::query` | `resource/mod.rs` | the resource's own row scoping: soft deletes, row-level visibility |
| `Resource::view_query` | `resource/mod.rs` | the detail page's query: `query` plus the relations the page reads off the record |
| `Resource::relations` | `resource/relation.rs` | the related resources rendered as tables on a record's detail and edit pages |
| `Resource::tenant_scope` | `resource/mod.rs` | the tenant predicate, derived from the model's `tenant_id` by default (`tenancy.rs`) |
| `Column` | `resource/column.rs` | a list column: its cell, its export text, its search, sort, width and the relations it reads |
| `Filter` | `resource/filter.rs` | a list filter: its predicate and its control |
| `Control` | `schema/fields/custom.rs` | a form field's input, beside the built-in text, choice and file controls |
| `Action` / `Resource::actions` | `resource/action.rs` | a mutation beyond CRUD, on a row or on the bulk selection |
| `Resource::can_*` | `resource/mod.rs` | authorization, default deny |
| `Resource::can_delete_any` | `resource/mod.rs` | whether delete is allowed at all: the delete chrome and the delete handlers' policy gate |
| `schema::OptionSource` | `schema/relationship.rs` | what a relationship select offers, and who may see it |
| `EmbeddedForm` | `tablo-macros` | the flat form map ↔ a typed embedded value |
| `RecordForm` / `NoForm` | `form.rs` | the typed value a form writes, and the form of a resource with none |
| `Uploader` | `upload.rs` | where a file field's bytes go |
| `Authenticator` | `auth.rs` | how credentials resolve to a `CurrentUser` |
| `Table::new` / `Table::new_split` | `resource/table/mod.rs` | row identity for keyed diffs and for action URLs |
| `panel::wired_table` | `panel/mod.rs` | the wired list table a page-owned table renders |

Row identity is two projections. `Table::new` takes one key projection and uses it for both halves:
the display key that drives keyed diffs and DOM ids, and the record key that handlers
resolve as the model's typed primary key. A table whose display projects a non-PK value
splits them with `Table::new_split(display, record, columns)`.

## Reactivity

The toolkit ships no client framework. Two Topcoat mechanisms cover the interactive parts:

- **`suspense`** streams a region's content after the first render. The resource list uses it so the
  page shell and skeleton arrive first and the table swaps in.
- **Shards** re-render a region in place. A table with `Table::live_search()` hands its chrome to
  the page's `TableSignals`: search, sort, filters, and pagination write signals, the shard re-renders
  the table, and Topcoat morphs the result in place so focus and scroll survive.

Page and layout guards do not run on a shard request, so a shard authorizes itself. Renders are
side-effect free and deterministic: no `HashMap` iteration, no `Utc::now()`, no random ids in a
streamed region.

## Assets

`tablo-ui` owns eleven browser scripts under `crates/tablo-ui/assets/`. They are loaded through
`asset!`, so they have no build step. Each one is wired to a constant in `tablo-ui/src/lib.rs`,
and a test guards the pairing: `cargo test -p xtask` runs `shell_assets_match_hook_contract`, which
fails when an asset is missing or a hook no longer appears in both its JavaScript and the Rust that
renders it. `asset!` does not read its source at compile time, so nothing else checks the JavaScript
side of that coupling.

`cargo xtask sync-topcoat-ui` re-vendors the components listed in `xtask::VENDORED_PRIMITIVES`
from `topcoat-ui-registry` and writes a content hash into each file header. Those files are never hand-edited;
`cargo xtask verify-topcoat-ui` fails on drift. Components in `components/composites/` are
Tablo's own and are never overwritten.

## Module map

```
crates/tablo-core/src/
  panel/      mod, build, state, url, gate, list, forms, write, actions/{bulk, delete,
              export, fetch, options}, detail, pages, relations, search, shell, headers
  resource/   mod, table/{mod,render,export}, column, declared, page, state, filter,
              relation, navigation, naming, commit
  schema/     mod, fields/{mod,builders,choice,custom,file,text}, lenses, options, layouts,
              tree, relationship, embedded, pk, validation
  auth, csrf, cursor, db, error, form, notification, page, query_term, tenancy,
  upload
```

The three largest modules split along the request shape rather than by type: `panel/` holds the
handlers, `resource/` holds what a resource declares and how a list renders it, and `schema/` holds
the form and detail-page declaration and its rendering.
