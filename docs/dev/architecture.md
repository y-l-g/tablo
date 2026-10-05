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
| `tablo-test` | `tablo-core`, `topcoat` | the in-memory HTTP client, `tablo::testing` |
| `tablo-build` | `topcoat` | `tailwind()`, the app's Tailwind build over Tablo's sources |
| `examples/showcase` | `tablo-core`, `tablo-ui`, `toasty`, `tablo-build` (build), `tablo-test` (dev) | the runnable admin and the integration tests |
| `examples/quickstart` | `tablo`, `tablo-build` | the smallest app, detached, built from outside the repo by `cargo xtask external-check` |

No library crate enables a database driver. Everything reaches the database through Toasty's `Db`
and `Executor`, and the app picks the driver: a `tablo` feature (`sqlite`, `postgresql`, `mysql`)
or a feature on its own `toasty` dependency. That is also why the `Uploader` and the
`Authenticator` are traits the app implements rather than crates the toolkit picks.

The derives name `tablo_core` when the app depends on it, and `tablo` otherwise; the facade
re-exports everything the generated code reaches. The Tailwind `links` plumbing is documented on
`tablo_build::tailwind()`.

## The layering

```
Panel  ──declares──▶  Resource  ──declares──▶  Table   (the list view)
   │                      │                  └▶  Schema  (forms, detail pages)
   │                      └──record fns─────▶  create / update / delete
   └──owns──▶ routes, Shell, the auth gate (the app owns the Router and the Db)
```

A `Panel` is an admin panel under one prefix. The app owns the router and the `Db` and mounts
the panel with `RouterBuilderPanelExt::panel`; one router mounts several panels at distinct
prefixes. Registering a `Resource` or a `Page` on a panel adds its routes and its sidebar entry.

Each mounted panel's state is one `PanelState` in the router's `Panels`. The panel's gate layer
puts it on every request under its prefix, so handlers read the request's panel, never a
router-wide singleton.

`Table` and `Schema` are declarations, not renderers. Mounting the panel calls each resource's
declarations once and serves the cached copy from every handler; a declaration that cannot render
fails the mount rather than a request.

## Requests

The list, detail, and write flows live where they are compiled: the `security.md` and
`policy-auth-tenancy.md` guide chapters describe the order of checks, and rustdoc on
`Resource`, `Table`, `Action`, and the panel handlers is the reference for each step. This file
does not repeat them.

## Extension points

The seams are the `Resource` traits (`query`, `view_query`, `relations`, `tenancy`, `policy`,
`actions`, `form`, `view`), the `Column`, `Filter`, `Control`, and `Action` traits, the
`EmbeddedForm` and `RecordForm` derives, the `Uploader`, and the `PanelUser` / `Authenticator`
pair with the `auth` helpers for app pages. Rustdoc on each item is the contract; the guide
shows idiomatic use.

## Reactivity and assets

The list streams inside a Topcoat `suspense` region and live tables re-render through the
framework-owned shard; resources declare no shards and page state belongs to the page. The
`tables.md` guide chapter and `Table::live_search` rustdoc describe the interaction. Browser
scripts are document-owned with no build step; the hook contract is guarded by
`cargo test -p xtask` (see `xtask/tests/it.rs`).
