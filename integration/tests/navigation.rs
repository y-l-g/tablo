//! A resource hidden from the sidebar: it stays mounted and answers at its URL under its policy,
//! and neither a sidebar entry nor the panel root leads to it.

use tablo::{
    DeclarationErrorKind, Page, ReadOnly, Resource, ResourceDef, Site, Table, TextColumn, lens,
    testing::{TestClient, mount_with_handle, nav_links},
};
use topcoat::{
    Result,
    context::Cx,
    view::{View, view},
};
use uuid::Uuid;

use crate::common::{mount, panel, refusal, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
}

/// Books anyone may read, listed in the sidebar.
struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("books")
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Book.title))))
    }
}

/// The same books under their own slug, kept out of the sidebar.
struct ArchiveResource;

impl Resource for ArchiveResource {
    type Model = Book;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("archive")
            .policy(ReadOnly)
            .navigation_hidden()
            .table(Table::new(TextColumn::new(lens!(Book.title))))
    }
}

/// The hidden list answers 200, the sidebar of its page and of the visible list's page holds the
/// visible entry alone, and the panel root skips the hidden resource declared first.
#[tokio::test]
async fn a_hidden_resource_answers_at_its_url_and_leaves_the_sidebar() {
    let db = test_db(toasty::models!(Book)).await;
    let (router, handle) = mount_with_handle(
        db,
        panel()
            .resource::<ArchiveResource>()
            .resource::<BookResource>(),
    )
    .expect("the panel mounts");
    let root = TestClient::new(&router).get("/admin").await;
    assert_eq!(
        root.headers().get(http::header::LOCATION).unwrap(),
        "/admin/books"
    );

    let client = TestClient::new(&router).panel(&handle);

    let archive = client.resource::<ArchiveResource>().list().await;
    assert_eq!(archive.status(), http::StatusCode::OK);
    assert_eq!(nav_links(archive.html()), ["/admin/books"]);

    let books = client.resource::<BookResource>().list().await;
    assert_eq!(books.status(), http::StatusCode::OK);
    assert_eq!(nav_links(books.html()), ["/admin/books"]);
}

/// Books nobody may list, kept out of the sidebar: the default policy denies.
struct SealedResource;

impl Resource for SealedResource {
    type Model = Book;
    type Form = tablo::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("sealed")
            .navigation_hidden()
            .table(Table::new(TextColumn::new(lens!(Book.title))))
    }
}

struct Home;

impl Page for Home {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx => "home" })
    }
}

/// Hiding a resource leaves its policy in force: a denied list answers 403 at its URL.
#[tokio::test]
async fn a_hidden_resource_still_answers_to_its_policy() {
    let db = test_db(toasty::models!(Book)).await;
    let (router, handle) = mount_with_handle(
        db,
        panel()
            .resource::<BookResource>()
            .resource::<SealedResource>(),
    )
    .expect("the panel mounts");
    let client = TestClient::new(&router).panel(&handle);

    let sealed = client.resource::<SealedResource>().list().await;
    assert_eq!(sealed.status(), http::StatusCode::FORBIDDEN);
}

/// A panel whose resources are all hidden has no root to redirect to, so it needs a home page:
/// without one the mount refuses, with one the root answers it.
#[tokio::test]
async fn a_panel_of_hidden_resources_needs_a_home_page() {
    let db = test_db(toasty::models!(Book)).await;
    let errors = refusal(mount(db.clone(), panel().resource::<ArchiveResource>()));
    assert_eq!(
        errors
            .into_iter()
            .map(|error| (error.resource, error.site, error.kind))
            .collect::<Vec<_>>(),
        [(None, Site::Panel, DeclarationErrorKind::NoRoot)]
    );

    let router =
        mount(db, panel().home::<Home>().resource::<ArchiveResource>()).expect("the panel mounts");
    let root = TestClient::new(&router).get("/admin").await;
    assert_eq!(root.status(), http::StatusCode::OK);
}
