//! What a relationship choice offers on its form and through its options endpoint: the rows the
//! related resource's list shows, nothing at all when it refuses the list, the stored row past the
//! option cap, and a server search for a searchable choice. A relationship filter over the same
//! source applies only those rows.

use tablo::{
    Ability, Allow, MAX_RELATIONSHIP_OPTIONS, NoForm, Resource, ResourceDef, Schema, SelectFilter,
    Table, TextColumn, lens,
    testing::{ErrorKind, IgnoredFilter, IgnoredReason, field_error, ignored_filters},
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{body_string, get, mount, panel, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Author {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    author_id: Uuid,
}

/// Lists every author and opens none named "Private".
fn closes_private(_cx: &Cx, ability: Ability<'_, Author>) -> bool {
    match ability {
        Ability::ViewAny => true,
        Ability::View(author) => author.name != "Private",
        _ => false,
    }
}

fn no_author(_cx: &Cx, _ability: Ability<'_, Author>) -> bool {
    false
}

/// Every author but the one named "Hidden".
struct AuthorResource;

impl Resource for AuthorResource {
    type Model = Author;
    type Form = NoForm<Author>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("authors")
            .policy(closes_private)
            .table(Table::new(TextColumn::new(lens!(Author.name))))
            .record_title(lens!(Author.name))
            .query(|_cx, query| query.filter(Author::fields().name().ne("Hidden")))
    }
}

/// No author at all.
struct LockedAuthorResource;

impl Resource for LockedAuthorResource {
    type Model = Author;
    type Form = NoForm<Author>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("locked-authors")
            .policy(no_author)
            .table(Table::new(TextColumn::new(lens!(Author.name))))
            .record_title(lens!(Author.name))
    }
}

#[derive(tablo::RecordForm)]
#[form(model = Book)]
struct BookForm {
    title: String,
    #[form(relationship = AuthorResource)]
    author_id: Uuid,
}

#[derive(tablo::RecordForm)]
#[form(model = Book)]
struct LockedBookForm {
    title: String,
    #[form(relationship = LockedAuthorResource)]
    author_id: Uuid,
}

struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = BookForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().slug("books").policy(Allow).table(
            Table::new(TextColumn::new(lens!(Book.title))).filters(SelectFilter::relationship::<
                AuthorResource,
            >(lens!(
                Book.author_id
            ))),
        )
    }
}

/// The same books, their author searched on the server past the cap.
struct SearchedBookResource;

impl Resource for SearchedBookResource {
    type Model = Book;
    type Form = BookForm;

    fn declare() -> ResourceDef<Self> {
        let c = BookForm::controls();
        ResourceDef::new()
            .slug("searched-books")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Book.title))))
            .form(Schema::new((c.title, c.author_id.searchable())))
    }
}

struct LockedBookResource;

impl Resource for LockedBookResource {
    type Model = Book;
    type Form = LockedBookForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("locked-books")
            .policy(Allow)
            .table(Table::new(TextColumn::new(lens!(Book.title))))
    }
}

/// A panel over the authors named `names`, and one book by the first.
async fn library(names: &[String]) -> (Router, Book, Vec<Author>) {
    let mut db: Db = test_db(toasty::models!(Author, Book)).await;
    let mut authors = Vec::new();
    for name in names {
        authors.push(
            toasty::create!(Author { name: name.clone() })
                .exec(&mut db)
                .await
                .unwrap(),
        );
    }
    let book = toasty::create!(Book {
        title: "Dune".to_string(),
        author_id: authors[0].id,
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(
        db,
        panel()
            .resource::<AuthorResource>()
            .resource::<LockedAuthorResource>()
            .resource::<BookResource>()
            .resource::<SearchedBookResource>()
            .resource::<LockedBookResource>(),
    )
    .expect("panel builds");
    (router, book, authors)
}

async fn edit_page(router: &Router, slug: &str, book: &Book) -> String {
    body_string(get(router, &format!("/admin/{slug}/{}/edit", book.id)).await).await
}

/// The choice offers each row the reader may view, by its key, and never a row `View` refuses,
/// even the one the record stores.
#[tokio::test]
async fn a_choice_offers_only_the_rows_its_source_lists() {
    let (router, book, authors) = library(&["Hidden".to_string(), "Ada".to_string()]).await;
    let html = edit_page(&router, "books", &book).await;
    let (hidden, ada) = (&authors[0], &authors[1]);
    assert!(
        html.contains(&format!("value=\"{}\"", ada.id)) && html.contains("Ada"),
        "{html}"
    );
    assert!(
        !html.contains("value=\"Ada\""),
        "a label is never a value: {html}"
    );
    assert!(
        !html.contains("Hidden") && !html.contains(&hidden.id.to_string()),
        "the stored row the source's query leaves out does not render: {html}"
    );
}

/// A choice offers a row its source's list shows though its policy refuses `View` of it, as the
/// list does.
#[tokio::test]
async fn a_choice_offers_a_row_view_refuses() {
    let (router, book, authors) = library(&["Ada".to_string(), "Private".to_string()]).await;
    let html = edit_page(&router, "books", &book).await;
    assert!(
        html.contains(&format!("value=\"{}\"", authors[1].id)),
        "{html}"
    );
}

/// A source that refuses the list leaks neither labels nor keys, and says the choice is
/// unavailable.
#[tokio::test]
async fn a_refused_source_offers_nothing_and_says_so() {
    let (router, book, authors) = library(&["Ada".to_string()]).await;
    let html = edit_page(&router, "locked-books", &book).await;
    assert!(
        !html.contains("Ada") && !html.contains(&authors[0].id.to_string()),
        "{html}"
    );
    assert_eq!(
        field_error(&html, "author_id"),
        Some(ErrorKind::Unavailable),
        "{html}"
    );
}

/// Past the cap the choice loads no option but keeps the one the record stores, and a searchable
/// one asks the server for the rest.
#[tokio::test]
async fn past_the_cap_a_choice_keeps_its_stored_row() {
    let names: Vec<String> = (0..=MAX_RELATIONSHIP_OPTIONS)
        .map(|i| format!("author-{i}"))
        .collect();
    let (router, book, authors) = library(&names).await;
    let stored = format!("value=\"{}\"", authors[0].id);

    let html = edit_page(&router, "books", &book).await;
    assert!(html.contains(&stored), "{html}");
    assert!(
        !html.contains(&format!("value=\"{}\"", authors[1].id)),
        "{html}"
    );

    let html = edit_page(&router, "searched-books", &book).await;
    assert!(
        html.contains(&stored) && html.contains("data-options-server"),
        "{html}"
    );
}

/// The options endpoint narrows a searchable choice by its query, and answers only a searchable
/// relationship choice the form declares.
#[tokio::test]
async fn the_option_search_narrows_only_a_searchable_choice() {
    let (router, _, _) = library(&["Ada".to_string(), "Grace".to_string()]).await;
    let response = get(
        &router,
        "/admin/searched-books/-/options?field=author_id&q=Ada",
    )
    .await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let html = body_string(response).await;
    assert!(html.contains("Ada") && !html.contains("Grace"), "{html}");

    for uri in [
        "/admin/searched-books/-/options?field=nope&q=Ada",
        "/admin/searched-books/-/options?q=Ada",
        "/admin/books/-/options?field=author_id&q=Ada",
    ] {
        assert_eq!(
            get(&router, uri).await.status(),
            http::StatusCode::BAD_REQUEST,
            "{uri}"
        );
    }
}

/// A search matching past the cap answers no option, and tells the choice's script to search the
/// server from now on.
#[tokio::test]
async fn a_search_past_the_cap_asks_for_more() {
    let names: Vec<String> = (0..=MAX_RELATIONSHIP_OPTIONS)
        .map(|i| format!("author-{i}"))
        .collect();
    let (router, _, _) = library(&names).await;
    let response = get(
        &router,
        "/admin/searched-books/-/options?field=author_id&q=",
    )
    .await;
    assert_eq!(response.status(), http::StatusCode::OK);
    assert_eq!(response.headers()["x-options-overflow"], "true");
    let html = body_string(response).await;
    assert!(!html.contains("author-"), "{html}");
}

/// An option's label reaches the page as text.
#[tokio::test]
async fn an_option_escapes_its_label() {
    let (router, _, _) = library(&["<script>x</script>".to_string()]).await;
    let html = body_string(
        get(
            &router,
            "/admin/searched-books/-/options?field=author_id&q=script",
        )
        .await,
    )
    .await;
    assert!(
        !html.contains("<script>x") && html.contains("&lt;script&gt;x"),
        "{html}"
    );
}

/// The author filter applies an author the source lists, and ignores one its query leaves out as
/// it ignores a key no author holds, under the option cap and past it.
#[tokio::test]
async fn a_relationship_filter_ignores_an_author_its_source_leaves_out() {
    for extra in [0, MAX_RELATIONSHIP_OPTIONS] {
        let names: Vec<String> = ["Ada", "Hidden"]
            .into_iter()
            .map(String::from)
            .chain((0..extra).map(|i| format!("author-{i}")))
            .collect();
        let (router, _, authors) = library(&names).await;
        let ignored = async |key: &str| {
            let url = format!("/admin/books?f.author_id={key}");
            ignored_filters(&body_string(get(&router, &url).await).await)
        };
        let invalid = |key: &str| {
            vec![IgnoredFilter {
                filter: format!("f.author_id={key}"),
                reason: IgnoredReason::Invalid,
            }]
        };
        let (ada, hidden) = (authors[0].id.to_string(), authors[1].id.to_string());
        let nobody = Uuid::new_v4().to_string();
        assert_eq!(ignored(&ada).await, [], "{extra} more authors");
        assert_eq!(
            ignored(&hidden).await,
            invalid(&hidden),
            "{extra} more authors"
        );
        assert_eq!(
            ignored(&nobody).await,
            invalid(&nobody),
            "{extra} more authors"
        );
    }
}
