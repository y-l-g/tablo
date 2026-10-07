# Tablo

**Admin toolkit for Rust**, server-rendered on **Topcoat** (UI and reactivity) and **Toasty**
(ORM). Declare a resource per model and get its list, create, edit, detail and delete pages, with
no SPA build step.

[![CI](https://github.com/y-l-g/tablo/actions/workflows/ci.yml/badge.svg)](https://github.com/y-l-g/tablo/actions/workflows/ci.yml) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Quick start

Run the showcase, a blog admin that exercises every feature:

```sh
cargo run -p showcase
# open http://localhost:3000/admin/users
```

Depend on the `tablo` facade, pick a database driver, and build the stylesheet with `tablo-build`
(the [first-panel chapter](docs/guide/src/first-panel.md) has the full manifest, including the
`topcoat` and `toasty` pins):

```toml
[dependencies]
tablo = { git = "https://github.com/y-l-g/tablo", features = ["sqlite"] }

[build-dependencies]
tablo-build = { git = "https://github.com/y-l-g/tablo" }
```

`examples/quickstart` is the smallest complete app; CI builds it from outside this repository.

A resource declares its model, its form and its list:

```rust
use tablo::prelude::*;

pub struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = NoForm<Self::Model>; // list-only: no create or edit pages

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(ReadOnly) // the default policy denies everything
            .table(Table::new(TextColumn::new(lens!(User.name)).searchable().sortable()).paginate(20))
    }
}
```

The panel registers it and frames its pages in the shell, and the app mounts the panel on its own
router, beside its other routes:

```rust
fn router(db: toasty::Db) -> topcoat::Result<Router> {
    Ok(Router::builder()
        .discover()
        .app_context(db)
        .panel(
            Panel::new("admin")
                .resource::<UserResource>()
                .auth(Auth::disabled()), // drop this line to require a login
        )?
        .build())
}
```

One router mounts several panels at distinct prefixes, each with its own resources, shell and
login. To add create and edit pages, give the resource a `#[derive(RecordForm)]` struct as its
`Form`, a `form()` schema, and a policy that allows `Create` and `Update`. The
[first panel](docs/guide/src/first-panel.md) chapter builds a complete app with forms and login.

## Documentation

- **[User guide](https://y-l.fr/tablo/nightly/guide/)**, from [`docs/guide/`](docs/guide/); build
  it locally with `mdbook build docs/guide`.
- **[API reference](https://y-l.fr/tablo/nightly/api/tablo/)**, the rustdoc of the
  workspace.
- [`examples/showcase/`](examples/showcase/): the runnable reference.
- [`CONTEXT.md`](CONTEXT.md): the project's vocabulary.
- [`docs/adr/`](docs/adr/): the design decisions, one record each.
- [`benchmarks/README.md`](benchmarks/README.md): the performance harness.
- Upstream: the [Toasty guide](https://tokio-rs.github.io/toasty/0.10.0/guide/) (queries,
  relations, migrations) and the [Topcoat docs](https://docs.rs/topcoat) (`view!`,
  `#[component]`, routing, cookies and sessions).

## Layout

`tablo-macros`, `tablo-ui` and `tablo-build` are leaves. `tablo-core` depends on `tablo-macros`
and `tablo-ui`, `tablo-test` on `tablo-core` and `topcoat`, and the `tablo` facade on
`tablo-core`, `tablo-ui` and (feature `testing`) `tablo-test`. An app names `tablo` under
`[dependencies]` and `tablo-build` under `[build-dependencies]`.

| Crate | Tablo and upstream dependencies | Contents |
| --- | --- | --- |
| `tablo` | `tablo-core`, `tablo-ui`, `tablo-test` (feature `testing`), `toasty` | the facade: `tablo_core` at its root, `ui`, `testing`, `prelude`, the driver features |
| `tablo-macros` | — | the `EmbeddedForm` and `RecordForm` derives |
| `tablo-ui` | `topcoat` | synced primitives, owned composites, `icons.rs` |
| `tablo-core` | `tablo-macros`, `tablo-ui`, `toasty` | Panel, Resource, Table, Schema, policy, auth, tenancy, upload |
| `tablo-test` | `tablo-core`, `topcoat` | the in-memory HTTP client and the response and HTML helpers, `tablo::testing` |
| `tablo-build` | `topcoat` | `tailwind()`, the app's Tailwind build over Tablo's sources |
| `examples/showcase` | `tablo-core`, `tablo-ui`, `toasty`, `tablo-build` (build), `tablo-test` (dev) | the runnable admin and the integration tests |
| `examples/guide` | `tablo`, `tablo-core`, `tablo-ui`, `tablo-build`, `toasty`, `topcoat` | the user guide's compiled companion: the anchored code `docs/guide/` includes |
| `examples/quickstart` | `tablo`, `tablo-build` | the smallest app, detached, built from outside the repository by `cargo xtask external-check` |
| `benchmarks/tablo` | `tablo-core`, `toasty`, `topcoat` | the list-path benchmark, a workspace member that is never published |

No library crate enables a database driver. Everything reaches the database through Toasty's `Db`
and `Executor`, and the app picks the driver: a `tablo` feature (`sqlite`, `postgresql`, `mysql`)
or a feature on its own `toasty` dependency. That is also why the `Uploader` and the
`Authenticator` are traits the app implements rather than crates the toolkit picks.

`tablo-core`'s top-level modules form four layers — foundations, the declaration model,
resources, and serving — and a module names its own layer and the ones below it, never one above.
`crates/tablo-core/tests/layers.rs` reads the sources and fails on an upward path; a new
top-level module joins a layer there. The seams an app extends are in the [extension
points](docs/guide/src/extension-points.md) chapter, and rustdoc on each item is the contract.

## Contributing

Small fixes can go straight to a pull request; open an issue first for larger changes.
[`AGENTS.md`](AGENTS.md) holds the rules; [`CONTRIBUTING.md`](CONTRIBUTING.md) covers the build,
the [CI gate set](CONTRIBUTING.md#the-gate-set) and the commit format.

## License

MIT — see [`LICENSE`](LICENSE).
