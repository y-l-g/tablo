//! Your first panel: the complete one-file admin.

/// The login page needs these two tables registered alongside the model.
// ANCHOR: book-app
use tablo::auth::{AdminUser, AuthSession, create_admin};
use tablo::prelude::*;
use toasty::Db;
use topcoat::{
    Result,
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
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().policy(ReadOnly)
    }
}

#[tokio::main]
pub async fn main() -> Result<()> {
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
    create_admin(&mut db, "admin@example.com", "secret", "Admin").await?;

    let router = Router::builder()
        .discover()
        .app_context(db)
        .panel(Panel::new("admin").resource::<BookResource>())?
        .build();

    topcoat::start(router).await?;
    Ok(())
}
// ANCHOR_END: book-app
