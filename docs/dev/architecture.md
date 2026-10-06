# Architecture

How the crates fit together and where the seams are. `CONTEXT.md` defines the vocabulary;
`docs/guide/` explains how to use the toolkit and rustdoc on each item documents its behaviour.

## Crates

`tablo-macros`, `tablo-ui` and `tablo-build` are leaves. `tablo-core` depends on `tablo-macros`
and `tablo-ui`, `tablo-test` on `tablo-core` and `topcoat`, and the `tablo` facade on
`tablo-core`, `tablo-ui` and (feature `testing`) `tablo-test`. An app names `tablo` under
`[dependencies]` and `tablo-build` under `[build-dependencies]`.

| Crate | Depends on | Contents |
| --- | --- | --- |
| `tablo` | `tablo-core`, `tablo-ui`, `tablo-test` (feature `testing`), `toasty` | the facade: `tablo_core` at its root, `ui`, `testing`, `prelude`, the driver features |
| `tablo-macros` | — | the `EmbeddedForm` and `RecordForm` derives |
| `tablo-ui` | `topcoat` | synced primitives, owned composites, `icons.rs` |
| `tablo-core` | `tablo-macros`, `tablo-ui`, `toasty` | Panel, Resource, Table, Schema, policy, auth, tenancy, upload |
| `tablo-test` | `tablo-core`, `topcoat` | the in-memory HTTP client and the response and HTML helpers, `tablo::testing` |
| `tablo-build` | `topcoat` | `tailwind()`, the app's Tailwind build over Tablo's sources |
| `examples/showcase` | `tablo-core`, `tablo-ui`, `toasty`, `tablo-build` (build), `tablo-test` (dev) | the runnable admin and the integration tests |
| `examples/guide` | `tablo`, `tablo-core`, `tablo-ui`, `tablo-build`, `toasty`, `topcoat` | the user guide's compiled companion: the anchored code `docs/guide/` includes |
| `examples/quickstart` | `tablo`, `tablo-build` | the smallest app, detached, built from outside the repo by `cargo xtask external-check` |
| `benchmarks/tablo` | `tablo-core`, `toasty`, `topcoat` | the list-path benchmark, a workspace member that is never published |

No library crate enables a database driver. Everything reaches the database through Toasty's `Db`
and `Executor`, and the app picks the driver: a `tablo` feature (`sqlite`, `postgresql`, `mysql`)
or a feature on its own `toasty` dependency. That is also why the `Uploader` and the
`Authenticator` are traits the app implements rather than crates the toolkit picks.

The derives name `tablo_core` when the app depends on it, and `tablo` otherwise; the facade
re-exports everything the generated code reaches. The Tailwind `links` plumbing is documented on
`tablo_build::tailwind()`.

## The layering

```
Panel  ──mounts──▶  ResourceDef  ──declares──▶  Table   (the list view)
   │        (Resource::declare)              └▶  Schema  (forms, detail pages)
   │                      Resource  ──record fns──▶  create / update / delete
   └──owns──▶ routes, Shell, the auth gate (the app owns the Router and the Db)
```

A `Panel` is an admin panel under one prefix. The app owns the router and the `Db` and mounts
the panel with `RouterBuilderPanelExt::panel`; one router mounts several panels at distinct
prefixes. Registering a `Resource` or a `Page` on a panel is declarative: mounting builds each
resource's `ResourceDef` once, binds its table, form and view to the app schema of the router's
`Db`, then claims its slug and adds its routes and its sidebar entry. Declarations are plain values
until then: a single-field path resolves against its own model when it is built, and an embedded
path or value, which names a column only the app schema knows, resolves when its declaration binds.

Each mounted panel's state is one `PanelState` in the router's `Panels`, holding the resources it
mounted (`Mounted<R>`, by resource type). The panel's gate layer puts it on every request under
its prefix, so handlers read the request's panel and its own copy of each resource, never a
router-wide singleton. A resource the request's panel does not mount has no def there.

`Table` and `Schema` are declarations, built once when the panel mounts. A request renders a table
as a `WiredTable`: the mounted declaration plus the row actions, policy verdicts and framing the
panel wires on for that request, so the declaration is shared, never copied or changed per
request. A declaration that cannot render fails the mount rather than a request.

### Inside `tablo-core`

The crate's top-level modules form four layers, and a module names its own layer and the ones
below it, never one above. `tests/layers.rs` reads the sources and fails on an upward path.

| Layer | Modules |
| --- | --- |
| foundations | `csrf`, `db`, `declaration`, `error`, `lens`, `naming`, `query_term`, `toasty_compat`, `topcoat_compat` |
| the declaration model | `form`, `navigation`, `policy`, `schema`, `table`, `tenancy` |
| resources | `resource` |
| serving | `auth`, `notification`, `page`, `panel`, `upload` |

Where a lower layer needs request state only the serving layer resolves, the serving layer
installs a function in the app context: `MountScope` finds the request panel's mounted resources
and `TenantSource` the session's tenant.

## Requests

The list, detail, and write flows live where they are compiled: the `security.md` and
`policy-auth-tenancy.md` guide chapters describe the order of checks, and rustdoc on
`Resource`, `Table`, `Action`, and the panel handlers is the reference for each step. This file
does not repeat them.

## Extension points

The seams are a resource's `ResourceDef` (`policy`, `tenancy`, `table`, `form`, `view`,
`relation`, `action`) and its `Resource` methods (`query`, `view_query`, the display hooks, and
the record fns), the `Column`, `Filter`, `Control`, and `Action` traits, the
`EmbeddedForm` and `RecordForm` derives, the `Uploader`, and the `PanelUser` / `Authenticator`
pair with the `auth` helpers for app pages. Rustdoc on each item is the contract; the guide
shows idiomatic use.

## Reactivity and assets

The list streams inside a Topcoat `suspense` region. Every table keeps its state in signals the
page reads, so a change reruns the page through its own route; ADR-0026 records the rule and the
`tables.md` guide chapter the interaction. The few remaining browser scripts are shell glue with no
build step; the hook contract is guarded by `cargo test -p xtask` (see `xtask/tests/it.rs`).
