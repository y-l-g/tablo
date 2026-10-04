# Your first panel

This chapter builds a complete admin in one file: a `Book` model, a resource for it, and the
`main` that serves it with a login page. `examples/quickstart` is this app with writes opened and the
stylesheet wired in.

## Dependencies

An app depends on the `tablo` facade, picks its database driver with a `tablo` feature, and builds
its stylesheet with `tablo-build`. It names `topcoat` and `toasty` directly for their macros, at the
revisions Tablo pins:

```toml
[dependencies]
tablo = { git = "https://github.com/y-l-g/tablo", features = ["sqlite"] }
topcoat = { git = "https://github.com/tokio-rs/topcoat", rev = "<the rev Tablo pins>", features = ["tailwind", "font", "font-fontsource", "asset"] }
toasty = { git = "https://github.com/tokio-rs/toasty", rev = "<the rev Tablo pins>", features = ["jiff"] }
jiff = "0.2"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
uuid = "1"

[build-dependencies]
tablo-build = { git = "https://github.com/y-l-g/tablo" }
```

The toolkit enables no driver itself: `sqlite`, `postgresql` and `mysql` each turn on Toasty's
driver of the same name. The `[workspace.dependencies]` of Tablo's `Cargo.toml` hold the pinned
revisions. `tablo` re-exports `tablo_core` at its root, so `tablo_core::X` in the other chapters is
`tablo::X` here, and the derives work with `tablo` as the only Tablo dependency.

## The app

```rust
use tablo::auth::{AdminUser, AuthSession, hash_password};
use tablo::prelude::*;
use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    router::{Router, RouterBuilderDiscoverExt},
};

/// The model: every column the panel renders comes from this type.
#[derive(Debug, Clone, toasty::Model)]
pub struct Book {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    pub title: String,
}

/// What the create and edit forms parse into.
#[derive(tablo::RecordForm)]
#[form(model = Book)]
pub struct BookForm {
    pub title: String,
}

pub struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = BookForm;

    // The default policy denies everything; this one opens the list and
    // the records, and nothing else.
    fn policy() -> impl Policy<Book> {
        ReadOnly
    }

    fn table() -> Table<Book> {
        Table::new(TextColumn::new(lens!(Book.title)).searchable().sortable())
    }

    fn form(_dx: &DeclCx) -> Schema {
        Schema::new(Field::text(Book::fields().title()))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut db = Db::builder()
        .models(toasty::models!(
            Book,
            // The built-in password login reads these two tables.
            AdminUser,
            AuthSession
        ))
        .connect("sqlite::memory:")
        .await?;
    db.push_schema().await?;

    // One account to sign in with.
    toasty::create!(AdminUser {
        email: "admin@example.com".to_string(),
        password_hash: hash_password("secret")?,
        display_name: "Admin".to_string(),
        active: true,
        created_at: jiff::Timestamp::now(),
    })
    .exec(&mut db)
    .await?;

    let router = Router::builder()
        .discover()
        .app_context(db)
        .panel(Panel::new("admin").resource::<BookResource>())?
        .build();

    topcoat::start(router).await?;
    Ok(())
}
```

Run it and sign in as `admin@example.com` / `secret`:

```sh
cargo run
# open http://127.0.0.1:3000/admin/books
```

## What each part does

- **`#[derive(RecordForm)]`** declares what a form submission parses into. Each field is named
  and typed like the model's field, so a renamed column fails to compile. See
  [Forms](./forms.md).
- **`impl Resource`** declares the admin for one model: its table, its form's controls, and its
  policy. A resource must name `Model` and `Form` and declare `table()`; every other item has a
  default. See [Resources](./resources.md).
- **`Db::builder().models(..)`** lists every model Toasty maps, including the two tables the
  built-in login uses. With authentication on, mounting the panel returns an error naming the
  missing models.
- **`db.push_schema()`** creates the tables, which suits a prototype. A production app runs
  `toasty-cli` migrations instead.
- **`Router::builder().discover()`** starts the app's own router: the app owns it, and the panel
  is one part of it. **`app_context(db)`** gives it the database every handler reads.
- **`Panel::new("admin")`** is a panel at `/admin`, framed in the shell — sidebar, topbar,
  toasts — by a layout it registers itself.
- **`resource::<BookResource>()`** registers the resource's routes under `/admin/books`, adds its
  sidebar entry, and, because the panel has no home page, makes `/admin` redirect to the book
  list. [Panel and routing](./panel-and-routing.md) lists every route.
- **`.panel(..)`** checks every declaration and mounts the panel, or returns an error naming the
  misdeclaration. **`build()`** returns the `Router`.
- **`topcoat::start`** serves the router on `HOST`:`PORT` (default `127.0.0.1:3000`) until Ctrl+C
  or `SIGTERM`.

## What the example leaves out

- **Writes.** `ReadOnly` allows only listing and viewing, so the create link is hidden, the create
  and edit pages answer 403, and rows show no Edit or Delete action.
  [Policy, auth, tenancy](./policy-auth-tenancy.md) opens them.
- **Styling.** The example registers no asset bundle, so pages render unstyled and without the
  shell's scripts, but every page and form works. The router's `.assets(..)` and the panel's
  `shell_assets` add the stylesheet, the font and the scripts; see
  [Panel and routing](./panel-and-routing.md#assets). The stylesheet itself comes from the build
  script below.

## The stylesheet

The panel's markup carries Tailwind classes, so the app generates a stylesheet that covers them.
The app owns `styles.css` at its package root: it imports `tailwindcss`, declares the theme tokens
(`examples/quickstart/styles.css` is a neutral set to start from), and names its own sources.

```css
@import "tailwindcss";
@source "./src/**/*.rs";
/* the theme tokens: --background, --foreground, --primary, ... */
```

`build.rs` runs the build:

```rust
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src");
    tablo_build::tailwind().expect("the Tailwind build runs");
}
```

`tablo_build::tailwind()` adds Tablo's own sources to the build, wherever Cargo unpacked them, and
writes `$OUT_DIR/tailwind.css`, which `tailwind::stylesheet!()` hands to `Panel::shell_assets`. The
stylesheet names no path into Tablo. The build downloads the Tailwind CLI on first run.
