# Your first panel

This chapter builds a complete admin in one file: a `Book` model, a resource for it, and the
`main` that serves it with a login page.

```rust
use tablo_core::auth::{AdminUser, hash_password};
use tablo_core::{Field, Panel, Resource, Schema, Table, TextColumn};
use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    router::{Slot, layout},
    view::View,
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
#[derive(tablo_core::RecordForm)]
#[form(model = Book)]
pub struct BookForm {
    pub title: String,
}

pub struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = BookForm;

    // Every policy predicate denies by default; this one opens the list.
    fn can_view_any(_cx: &Cx) -> bool {
        true
    }

    fn table(_cx: &Cx) -> Table<Book> {
        Table::new(
            |b: &Book| b.id.to_string(),
            TextColumn::r#for(Book::fields().title(), |b: &Book| b.title.clone())
                .searchable()
                .sortable(),
        )
    }

    fn form(_cx: &Cx) -> Schema {
        Schema::new(Field::text(Book::fields().title()))
    }
}

/// Frames every page under `/admin` in the panel's shell: sidebar, topbar, toasts.
#[layout("/admin")]
async fn admin_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    Panel::layout_shell(cx, slot).await
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut db = Db::builder()
        .models(toasty::models!(
            Book,
            // The built-in password login reads these two tables.
            tablo_core::auth::AdminUser,
            tablo_core::auth::AuthSession
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
        tenant_id: None,
        created_at: jiff::Timestamp::now(),
    })
    .exec(&mut db)
    .await?;

    let router = Panel::new("admin")
        .app_context(db)
        .resource::<BookResource>()
        .build()?;

    topcoat::start(router).await?;
    Ok(())
}
```

Run it and sign in as `admin@example.com` / `secret`:

```sh
cargo run
# open http://127.0.0.1:3000/admin/books
```

The example depends on `tablo-core`, `topcoat` (its default features include the server),
`toasty` with the `sqlite` and `jiff` features, `jiff` for `Timestamp::now()`, `tokio` with
`macros` and `rt-multi-thread`, and `uuid`. The workspace `Cargo.toml` pins the revisions the
toolkit is tested with.

## What each part does

- **`#[derive(RecordForm)]`** declares what a form submission parses into. Each field is named
  and typed like the model's field, so a renamed column fails to compile. See
  [Forms](./forms.md).
- **`impl Resource`** declares the admin for one model: its table, its form's controls, and its
  policy. A resource must name `Model` and `Form` and declare `table()`; every other item has a
  default. See [Resources](./resources.md).
- **`#[layout("/admin")]`** frames the panel's pages. `Panel::layout_shell` renders the whole
  document around them; the layout's path must match the panel's prefix.
- **`Db::builder().models(..)`** lists every model Toasty maps, including the two tables the
  built-in login uses. With authentication on, `Panel::build` panics if they are missing.
- **`db.push_schema()`** creates the tables, which suits a prototype. A production app runs
  `toasty-cli` migrations instead.
- **`Panel::new("admin")`** mounts the panel at `/admin`. **`app_context(db)`** gives it the
  database; `Panel::build` fails without one.
- **`resource::<BookResource>()`** registers the resource's routes under `/admin/books`, adds its
  sidebar entry, and, because the panel has no home page, makes `/admin` redirect to the book
  list. [Panel and routing](./panel-and-routing.md) lists every route.
- **`build()`** checks every declaration and returns the `Router`, or an error naming the
  misdeclaration.
- **`topcoat::start`** serves the router on `HOST`:`PORT` (default `127.0.0.1:3000`) until Ctrl+C
  or `SIGTERM`.

## What the example leaves out

- **Writes.** Only `can_view_any` is allowed, so the create link is hidden, the create and edit
  pages answer 403, and rows show no Edit or Delete action.
  [Policy, auth, tenancy](./policy-auth-tenancy.md) opens them.
- **Styling.** The example registers no asset bundle, so pages render unstyled and without the
  shell's scripts, but every page and form works. `Panel::assets` and `Panel::shell_assets` add
  the stylesheet, the font and the scripts; see
  [Panel and routing](./panel-and-routing.md#assets).
