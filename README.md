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

A resource declares its model, its form and its list:

```rust
pub struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = NoForm<Self::Model>; // list-only: no create or edit pages

    fn can_view_any(_cx: &Cx) -> bool {
        true // every policy predicate denies until allowed
    }

    fn table(_cx: &Cx) -> Table<User> {
        Table::new(
            |u: &User| u.id.to_string(),
            TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone())
                .searchable()
                .sortable(),
        )
        .paginate(20)
    }
}
```

The panel registers it, and the app's layout delegates to the panel's shell:

```rust
#[layout("/admin")]
async fn admin_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    Panel::layout_shell(cx, slot).await
}

fn router(db: toasty::Db) -> Router {
    Panel::new("admin")
        .app_context(db)
        .resource::<UserResource>()
        .auth(Auth::disabled()) // drop this line to require a login
        .build()
        .expect("panel builds")
}
```

To add create and edit pages, give the resource a `#[derive(RecordForm)]` struct as its `Form`, a
`form()` schema, and `can_create` / `can_update`. The
[first panel](docs/guide/src/first-panel.md) chapter builds a complete app with forms and login.

## Documentation

- **[User guide](https://y-l.fr/tablo/nightly/guide/)**, from [`docs/guide/`](docs/guide/); build
  it locally with `mdbook build docs/guide`.
- **[API reference](https://y-l.fr/tablo/nightly/api/tablo_core/)**, the rustdoc of the
  workspace.
- [`examples/showcase/`](examples/showcase/): the runnable reference.
- [`CONTEXT.md`](CONTEXT.md): the project's vocabulary.
- [`docs/adr/`](docs/adr/): the design decisions, one record each.
- [`docs/dev/architecture.md`](docs/dev/architecture.md): how the crates and a request fit
  together.
- [`benchmarks/README.md`](benchmarks/README.md): the performance harness.
- Upstream: the [Toasty guide](https://tokio-rs.github.io/toasty/0.10.0/guide/) (queries,
  relations, migrations) and the [Topcoat docs](https://docs.rs/topcoat) (`view!`,
  `#[component]`, routing, cookies and sessions).

## Contributing

Small fixes can go straight to a pull request; open an issue first for larger changes.
[`CONTRIBUTING.md`](CONTRIBUTING.md) covers the build, the [CI gate
set](CONTRIBUTING.md#the-gate-set) and the commit rules; [`AGENTS.md`](AGENTS.md) is the short
version for agents.

## License

MIT — see [`LICENSE`](LICENSE).
