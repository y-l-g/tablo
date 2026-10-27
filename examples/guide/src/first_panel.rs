//! Your first panel: the complete one-file admin.

// ANCHOR: book-app
use tablo::{auth::create_admin, prelude::*};
use toasty::Db;
use topcoat::{
    Result,
    router::{Router, RouterBuilderDiscoverExt},
};

/// The model: every column the panel renders comes from this type, and
/// `ModelForm` writes `BookForm`, what the create and edit forms parse into.
#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns, tablo::ModelForm)]
pub struct Book {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
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
        // `Book`, and the tables Tablo ships: the login's users and sessions among them.
        .models(tablo::models!(Book))
        .connect("sqlite::memory:")
        .await?;
    db.push_schema().await?;

    // One account to sign in with.
    create_admin(&mut db, "admin@example.com", "secret", "Admin").await?;

    let router = Router::builder()
        .discover()
        .with(db)
        .panel(Panel::new("admin").resource::<BookResource>())?
        .build();

    topcoat::start(router).await?;
    Ok(())
}
// ANCHOR_END: book-app
