# Tablo

Admin toolkit for Rust, server-rendered on [Topcoat](https://docs.rs/topcoat) (UI and reactivity)
and [Toasty](https://docs.rs/toasty) (ORM). Declare a resource per model and get its list, create,
edit, detail and delete pages, with no SPA build step.

```sh
cargo add tablo --features sqlite
cargo add --build tablo-build
```

`sqlite`, `postgresql` and `mysql` each turn on Toasty's driver of the same name; the toolkit
enables none of them itself, and the `testing` feature adds the in-memory client `tablo::testing`.
An app also names `topcoat` and `toasty` for their macros — the
[first panel](https://y-l.fr/tablo/nightly/guide/first-panel.html) chapter has the full manifest.

A resource declares its model, its form and the rest of its definition:

```rust
use tablo::prelude::*;

#[derive(Debug, Clone, toasty::Model)]
pub struct Book {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub title: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Book)]
pub struct BookForm {
    pub title: String,
}

pub struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = BookForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().policy(Allow)
    }
}
```

The panel serves it on the app's Topcoat router, beside its other routes:

```rust
let router = Router::builder()
    .discover()
    .app_context(db)
    .panel(Panel::new("admin").resource::<BookResource>())?
    .build();
```

## Documentation

- [User guide](https://y-l.fr/tablo/nightly/guide/), starting at "Your first panel".
- [`examples/quickstart`](../../examples/quickstart), the smallest complete app.
- [API reference](https://docs.rs/tablo).
