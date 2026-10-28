# Tablo

**Admin toolkit for Rust**, server-rendered on **Topcoat** (UI and reactivity) and **Toasty**
(ORM). Declare a resource per model and get its list, create, edit, detail and delete pages, with
no SPA build step.

Tablo is experimental: until 1.0, a minor version may break the API.

[![CI](https://github.com/y-l-g/tablo/actions/workflows/ci.yml/badge.svg)](https://github.com/y-l-g/tablo/actions/workflows/ci.yml) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/y-l-g/tablo/blob/master/LICENSE)

## Quick start

Tablo needs Rust 1.99 or newer; every crate declares that as its `rust-version`.

Run the showcase, a blog's admin panel that exercises every feature, through the Topcoat CLI:

```sh
cargo install topcoat-cli --version 0.11 --locked
topcoat dev -p showcase
# open http://localhost:3000/admin/users
```

`topcoat dev` builds the showcase, bundles its assets beside the binary, serves it, and rebuilds it
when a source file changes. `cargo run -p showcase` alone stops with an error: it finds no asset
bundle.

Depend on the `tablo` facade, pick a database driver, and build the stylesheet with `tablo-build`
(the [first-panel chapter](https://y-l.fr/tablo/nightly/guide/first-panel.html) has the full manifest, including the
`topcoat` and `toasty` pins):

```sh
cargo add tablo --features sqlite
cargo add --build tablo-build
```

`sqlite`, `postgresql`, `mysql` and `turso` each turn on Toasty's driver of the same name; the
toolkit enables none of them itself, and the `testing` feature adds the in-memory client
`tablo::testing`.

`examples/quickstart` is the smallest complete app; CI builds it from outside this repository.

A model derives the table and detail page it reads as by default, and a resource names it, its
form and its policy:

```rust
use tablo::prelude::*;

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
pub struct User {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub name: String,
    #[default(jiff::Timestamp::now())]
    pub created_at: jiff::Timestamp,
}

pub struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = NoForm<Self::Model>; // list-only: no create or edit pages

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().policy(ReadOnly) // the default policy denies everything
    }
}
```

The panel registers it and frames its pages in the shell, and the app mounts the panel on its own
router, beside its other routes:

```rust
fn router(db: toasty::Db) -> topcoat::Result<Router> {
    Ok(Router::builder()
        .discover()
        .with(db)
        .panel(
            Panel::new("admin")
                .resource::<UserResource>()
                .auth(Auth::disabled()), // drop this line to require a login
        )?
        .build())
}
```

One router mounts several panels at distinct prefixes, each with its own resources, shell and
login. To add create and edit pages, derive `tablo::ModelForm` on the model, name the `UserForm` it
writes as the resource's `Form`, and give it a policy that allows `Create` and `Update`. The
[first panel](https://y-l.fr/tablo/nightly/guide/first-panel.html) chapter builds a complete app with forms and login.

## Documentation

- **[User guide](https://y-l.fr/tablo/nightly/guide/)**, from the [guide source](https://github.com/y-l-g/tablo/blob/master/docs/guide/); build it
  locally with `mdbook build docs/guide`.
- **[API reference](https://y-l.fr/tablo/nightly/api/tablo/)**, the rustdoc of the
  workspace.
- [Showcase](https://github.com/y-l-g/tablo/blob/master/examples/showcase/): the runnable reference.
- [Vocabulary](https://github.com/y-l-g/tablo/blob/master/CONTEXT.md): the project's terms.
- [Design decisions](https://github.com/y-l-g/tablo/blob/master/docs/adr/): one record each.
- Upstream: the [Toasty guide](https://tokio-rs.github.io/toasty/0.11.0/guide/) (queries,
  relations, migrations) and the [Topcoat docs](https://docs.rs/topcoat) (`view!`,
  `#[component]`, routing, cookies and sessions).

## Contributing

Small fixes can go straight to a pull request; open an issue first for larger changes.
The [contributing guide](https://github.com/y-l-g/tablo/blob/master/CONTRIBUTING.md) covers the layout, the checks and the rules.

## License

MIT — see [LICENSE](https://github.com/y-l-g/tablo/blob/master/LICENSE).
