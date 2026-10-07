//! The guide's executed snippets: the Testing chapter's suite and the data-access background job.

use guide::{
    data_access::count_drafts,
    first_panel::{Book, BookResource},
    models::{Author, Comment, Post, Seo},
};
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

// ANCHOR: testing-mount-error
#[tokio::test]
async fn the_panel_refuses_a_db_without_the_auth_models() {
    let db = Db::builder()
        .models(toasty::models!(Book))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    let error = Router::builder()
        .discover()
        .app_context(db)
        .panel(Panel::new("admin").resource::<BookResource>())
        .err()
        .expect("auth is on, and the Db does not register its models");

    let refusal = error
        .downcast_ref::<tablo::MountError>()
        .expect("a declaration mistake");
    assert!(matches!(
        refusal.errors()[0].kind,
        tablo::DeclarationErrorKind::MissingAuthModels { .. }
    ));
}
// ANCHOR_END: testing-mount-error

/// A blog with two drafts and one published post for `tenant`, and one draft for another.
async fn seeded_blog() -> (Db, uuid::Uuid) {
    let tenant = uuid::Uuid::new_v4();
    let other = uuid::Uuid::new_v4();
    let mut db = Db::builder()
        .models(toasty::models!(Author, Post, Comment))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    db.push_schema().await.expect("push the schema");
    let author = toasty::create!(Author {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .expect("seed an author");
    for (tenant_id, status) in [
        (tenant, "draft"),
        (tenant, "draft"),
        (tenant, "published"),
        (other, "draft"),
    ] {
        toasty::create!(Post {
            tenant_id: TenantId::from(tenant_id),
            title: "A post".to_string(),
            body: "body".to_string(),
            status: status.to_string(),
            featured: false,
            created_at: "2024-01-15T09:30:00Z".parse::<jiff::Timestamp>().unwrap(),
            author_id: author.id,
            seo: Seo {
                title: "A post".to_string(),
                description: String::new(),
            },
        })
        .exec(&mut db)
        .await
        .expect("seed a post");
    }
    (db, tenant)
}

/// The data-access chapter's job runs against the panel the app mounts, scoped to its tenant.
#[tokio::test]
async fn a_background_job_counts_the_drafts_of_its_tenant() {
    let (db, tenant) = seeded_blog().await;
    let drafts = count_drafts(&db, tenant).await.expect("the job runs");
    assert_eq!(drafts, 2, "two of the four posts are this tenant's drafts");
}
