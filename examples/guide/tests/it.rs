//! The guide's executed snippets: the Testing chapter's suite, the data-access background job, and
//! the Forms chapter's many-to-many models, which only mounting checks.

use guide::{
    data_access::{admin_router, count_drafts},
    first_panel::{Book, BookResource},
    models::{Author, Comment, Post, PostStatus, Seo},
};
use tablo::{
    auth::create_admin,
    prelude::*,
    testing::{TestClient, mount, mount_with_handle, refusal},
};
use toasty::Db;

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

async fn first_book(db: &Db) -> Book {
    let mut db = db.clone();
    let books = Book::all().exec(&mut db).await.expect("list books");
    books.into_iter().next().expect("one book")
}

// ANCHOR: testing-no-delete
#[tokio::test]
async fn books_cannot_be_deleted() {
    let mut db = seeded_db().await; // your fixture: an in-memory database with rows
    let book = first_book(&db).await;
    // `mount_with_handle` builds the router an app does, beside its own pages and routes, and
    // keeps the mounted panel, which the client reads its URLs from.
    let (router, panel) =
        mount_with_handle(db.clone(), Panel::new("admin").resource::<BookResource>())
            .expect("mount the panel");
    let admin = create_admin(&mut db, "admin@example.com", "secret", "Admin")
        .await
        .expect("seed the admin");
    // `sign_in` records a session for `admin`, as a successful login does.
    let client = TestClient::new(&router)
        .sign_in(&db, "admin", &admin)
        .await
        .panel(&panel);
    let books = client.resource::<BookResource>();

    let list = books.list().await;
    assert_eq!(list.status(), 200);
    assert_eq!(list.rows().len(), 1);

    // `delete` posts the confirmed delete to the record's route, with the CSRF pair.
    let reply = books.delete(&book).submit().await;
    assert_eq!(reply.status(), 403); // the policy does not allow `DeleteAny`

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
    // Auth is on, and the `Db` does not register its models.
    let mistakes = refusal(mount(db, Panel::new("admin").resource::<BookResource>()));
    assert!(matches!(
        mistakes[0].kind,
        tablo::DeclarationErrorKind::MissingAuthModels { .. }
    ));
}
// ANCHOR_END: testing-mount-error

/// A blog with two drafts and one published post for `tenant`, and one draft for another.
async fn seeded_blog() -> (Db, uuid::Uuid) {
    let tenant = uuid::Uuid::new_v4();
    let other = uuid::Uuid::new_v4();
    let mut db = Db::builder()
        .models(toasty::models!(
            Author,
            Post,
            Comment,
            tablo::auth::AdminUser,
            tablo::auth::AuthSession
        ))
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
        (tenant, PostStatus::Draft),
        (tenant, PostStatus::Draft),
        (tenant, PostStatus::Published),
        (other, PostStatus::Draft),
    ] {
        toasty::create!(Post {
            tenant_id: TenantId::from(tenant_id),
            title: "A post".to_string(),
            body: "body".to_string(),
            status,
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
    let (_router, admin) = admin_router(db).expect("the panel mounts");
    let drafts = count_drafts(&admin, tenant).await.expect("the job runs");
    assert_eq!(drafts, 2, "two of the four posts are this tenant's drafts");
}

/// The many-to-many field and relation the Forms chapter shows mount over their join model.
#[tokio::test]
async fn the_many_to_many_snippets_mount() {
    use guide::forms::{Article, ArticleResource, Tag, TagResource, Tagging};

    let db = Db::builder()
        .models(toasty::models!(Article, Tag, Tagging))
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    db.push_schema().await.expect("push the schema");
    mount(
        db,
        Panel::new("admin")
            .auth(Auth::disabled())
            .resource::<ArticleResource>()
            .resource::<TagResource>(),
    )
    .expect("the panel mounts");
}
