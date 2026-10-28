//! A resource hidden from the sidebar: it stays mounted and answers at its URL, and no sidebar
//! entry links to it.

use tablo::{
    ReadOnly, Resource, ResourceDef, Table, TextColumn, lens,
    testing::{TestClient, mount_with_handle, nav_links},
};
use uuid::Uuid;

use crate::common::{panel, test_db};

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

/// The hidden list answers 200, and the sidebar of its page and of the visible list's page holds
/// the visible entry alone.
#[tokio::test]
async fn a_hidden_resource_answers_at_its_url_and_leaves_the_sidebar() {
    let db = test_db(toasty::models!(Book)).await;
    let (router, handle) = mount_with_handle(
        db,
        panel()
            .resource::<BookResource>()
            .resource::<ArchiveResource>(),
    )
    .expect("the panel mounts");
    let client = TestClient::new(&router).panel(&handle);

    let archive = client.resource::<ArchiveResource>().list().await;
    assert_eq!(archive.status(), http::StatusCode::OK);
    assert_eq!(nav_links(archive.html()), ["/admin/books"]);

    let books = client.resource::<BookResource>().list().await;
    assert_eq!(books.status(), http::StatusCode::OK);
    assert_eq!(nav_links(books.html()), ["/admin/books"]);
}
