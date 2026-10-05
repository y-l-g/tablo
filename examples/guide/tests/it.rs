//! The Testing chapter's suite: the guide's example as a real test.

use guide::first_panel::{Book, BookResource};
use tablo::{
    prelude::*,
    testing::{TestClient, form_body},
};
use toasty::Db;
use topcoat::router::{Router, RouterBuilderDiscoverExt};

async fn seeded_db() -> Db {
    let mut db = Db::builder()
        .models(toasty::models!(
            Book,
            tablo::auth::AdminUser,
            tablo::auth::AuthSession
        ))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    db.push_schema().await.expect("push the schema");
    toasty::create!(Book {
        title: "Emma".to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed one book");
    db
}

async fn first_book_id(db: &Db) -> uuid::Uuid {
    let mut db = db.clone();
    let books = Book::all().exec(&mut db).await.expect("list books");
    books.into_iter().next().expect("one book").id
}

// ANCHOR: testing-no-delete
#[tokio::test]
async fn books_cannot_be_deleted() {
    let mut db = seeded_db().await; // your fixture: an in-memory database with rows
    let id = first_book_id(&db).await;
    let router = Router::builder()
        .discover()
        .app_context(db.clone())
        .panel(
            Panel::new("admin")
                .resource::<BookResource>()
                .auth(Auth::disabled()),
        )
        .expect("mount the panel")
        .build();
    let client = TestClient::new(&router);

    assert_eq!(client.get("/admin/books").await.status(), 200);

    // A POST needs the CSRF cookie and a matching `csrf_token` field.
    let token = uuid::Uuid::new_v4().to_string();
    let response = client
        .csrf(&token)
        .post_form(
            &format!("/admin/books/{id}/delete"),
            form_body(&[("csrf_token", &token), ("confirm", "1")]),
        )
        .await;
    assert_eq!(response.status(), 403); // the policy does not allow `DeleteAny`

    let books = Book::all().exec(&mut db).await.expect("list books");
    assert_eq!(books.len(), 1); // nothing was deleted
}
// ANCHOR_END: testing-no-delete
