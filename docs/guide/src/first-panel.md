# Your first panel

A complete panel: one Toasty model, one `Resource` over it, and the `main` that mounts them and
serves the admin.

```rust
use tablo_core::auth::{AdminUser, hash_password};
use tablo_core::{Field, Panel, Resource, Schema, Table, TextColumn};
use toasty::Db;
use topcoat::{Result, context::Cx};

/// The persisted model: every column the panel renders comes from this type.
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

    // Policy defaults to deny, so the list answers 403 without this.
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

#[tokio::main]
async fn main() -> Result<()> {
    let mut db = Db::builder()
        .models(toasty::models!(
            Book,
            // The shipped password auth reads these two tables, and
            // `Panel::build` asserts both are registered.
            tablo_core::auth::AdminUser,
            tablo_core::auth::AuthSession
        ))
        .connect("sqlite::memory:")
        .await?;

    db.push_schema().await?;

    // One account to sign in with; the hash is an Argon2id PHC string.
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

    // `app_context(db)` installs the Db `Panel::build` requires.
    let router = Panel::new("admin")
        .app_context(db)
        .resource::<BookResource>()
        .build()?;

    topcoat::start(router).await?;
    Ok(())
}
```

```sh
cargo run
# then open http://127.0.0.1:3000/admin/books and sign in as admin@example.com / secret
```

## The wiring

The one required call the other chapters leave implicit is `app_context(db)`: the panel reads its
pooled `Db` from the app context, and `Panel::build` returns an error without one.

- `Db::builder().models(..)` names every model Toasty maps. The shipped password auth reads
  `tablo_core::auth::AdminUser` and `tablo_core::auth::AuthSession`, so both are listed; with auth
  on, `Panel::build` asserts they are there, and the panel serves its login page at
  `/admin/login` ([Policy, auth, tenancy](./policy-auth-tenancy.md)).
- `db.push_schema().await` creates the tables for a prototype. Production uses `toasty-cli`
  migrations ([Data access](./data-access.md)).
- `Panel::new("admin")` mounts the panel at `/admin`. `resource::<BookResource>()` registers the
  resource's list at `{prefix}/{slug}`, derives its sidebar entry from the same slug, and points
  the panel root at the first declared resource, so `BookResource` answers on `/admin/books`. The
  other routes — create, edit, detail, delete, bulk delete, export, relationship options — come
  from the same registration ([Panel and routing](./panel-and-routing.md)).
- `app_context(db)` registers the pooled `Db` on the panel's app context. `Panel::build` requires
  it: without that call it returns `Err` (`Panel::build requires a Db via app_context`) instead of a
  router. `build` also uses that same database to run its declaration checks, with no request in
  hand.
- `build()?` returns the `Router`, or an error naming the misdeclaration it found — a duplicate or
  malformed resource slug, a malformed panel prefix, a form that disagrees with the resource's
  `form()` schema ([Resources](./resources.md)).
- `topcoat::start(router)` binds `HOST` and `PORT` (`127.0.0.1:3000` when unset) and serves until
  Ctrl+C or `SIGTERM`.

The example sets `can_view_any` and leaves the other `can_*` predicates at their deny default: no
Create link renders, the create and edit pages answer 403, and no row carries an Edit or Delete
control. Declaring those is [Policy, auth, tenancy](./policy-auth-tenancy.md); the columns, filters
and export are [Tables](./tables.md); the form is [Forms](./forms.md).

The example also registers no asset bundle, so the shell falls back to `topcoat::dev::script()` and
its theme script: the pages render and the forms submit, and `Panel::assets(..)` with
`Panel::shell_assets(..)` adds the generated stylesheet, the font and the client scripts
([Panel and routing](./panel-and-routing.md)).
