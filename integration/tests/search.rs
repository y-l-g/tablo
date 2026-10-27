//! The global search over resources the showcase does not build: a record the policy keeps from
//! its pages, a field the request may not view, a resource without a detail page, a resource and
//! a panel left out of the search, records the policy refuses ahead of the ones it allows, and the
//! results shard on one or two panels without sign-in.

use tablo::{
    Ability, Auth, Detail, FieldAccess, GlobalSearch, NoForm, Panel, Resource, ResourceDef,
    RouterBuilderPanelExt, lens,
    testing::{SearchGroup, search_box, search_results},
};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Router, RouterBuilderDiscoverExt},
};
use uuid::Uuid;

use crate::common::{TestClient, body_string, form_body, mount, panel, test_db};

const RESULTS_SHARD: &str = "/_topcoat/runtime/shards/tablo-search";

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    isbn: String,
    notes: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Book)]
struct BookForm {
    title: String,
    isbn: String,
    notes: String,
}

/// Refuses `View` on a book titled `Hidden …` and `Update` on one titled `Locked …`.
fn book_policy(_cx: &Cx, ability: Ability<'_, Book>) -> bool {
    match ability {
        Ability::View(book) => !book.title.starts_with("Hidden"),
        Ability::Update(book) => !book.title.starts_with("Locked"),
        _ => true,
    }
}

/// No request views a book's notes.
fn nobody(_cx: &Cx) -> bool {
    false
}

struct BookResource;

impl Resource for BookResource {
    type Model = Book;
    type Form = BookForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy_fn(book_policy)
            .field_access(lens!(Book.notes), FieldAccess::new().view(nobody))
            .record_title(lens!(Book.title))
            .global_search(
                GlobalSearch::new()
                    .detail("ISBN", lens!(Book.isbn))
                    .detail("Notes", lens!(Book.notes)),
            )
    }
}

/// The same books without a detail page, so a result opens the edit page.
struct ShelfResource;

impl Resource for ShelfResource {
    type Model = Book;
    type Form = BookForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("shelf")
            .plural_label("Shelf")
            .policy_fn(book_policy)
            .detail(Detail::empty())
            .record_title(lens!(Book.title))
    }
}

/// The same books, left out of the search.
struct ArchiveResource;

impl Resource for ArchiveResource {
    type Model = Book;
    type Form = NoForm<Book>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("archive")
            .plural_label("Archive")
            .policy_fn(|_cx, _ability: Ability<'_, Book>| true)
            .without_global_search()
    }
}

fn books_panel() -> Panel {
    books_panel_at("admin")
}

fn books_panel_at(prefix: &str) -> Panel {
    Panel::new(prefix)
        .auth(Auth::disabled())
        .resource::<BookResource>()
        .resource::<ShelfResource>()
        .resource::<ArchiveResource>()
}

async fn db() -> Db {
    test_db(toasty::models!(Book)).await
}

async fn book(db: &Db, title: &str, isbn: &str, notes: &str) -> Book {
    toasty::create!(Book {
        title: title.to_string(),
        isbn: isbn.to_string(),
        notes: notes.to_string(),
    })
    .exec(&mut db.clone())
    .await
    .unwrap()
}

async fn search(router: &Router, term: &str) -> Vec<SearchGroup> {
    let response = TestClient::new(router)
        .get(&format!("/admin/-/search?{}", form_body(&[("q", term)])))
        .await;
    assert_eq!(response.status(), 200, "the results page answers {term:?}");
    search_results(&body_string(response).await)
}

/// The `(title, href)` of each result under `label`.
fn results(groups: &[SearchGroup], label: &str) -> Vec<(String, String)> {
    groups
        .iter()
        .filter(|group| group.label == label)
        .flat_map(|group| {
            group
                .results
                .iter()
                .map(|result| (result.title.clone(), result.href.clone()))
        })
        .collect()
}

#[tokio::test]
async fn a_result_opens_the_detail_page_or_without_one_the_edit_page() {
    let db = db().await;
    let dune = book(&db, "Dune", "978-0441013593", "").await;
    book(&db, "Hidden Dune", "1", "").await;
    let locked = book(&db, "Locked Dune", "2", "").await;
    let router = mount(db.clone(), books_panel()).expect("the panel mounts");

    let groups = search(&router, "dune").await;

    assert_eq!(
        groups.iter().map(|g| g.label.as_str()).collect::<Vec<_>>(),
        ["Books", "Shelf"],
        "the archive is left out"
    );
    assert_eq!(
        results(&groups, "Books"),
        [
            ("Dune".to_string(), format!("/admin/books/{}", dune.id)),
            (
                "Locked Dune".to_string(),
                format!("/admin/books/{}", locked.id)
            ),
        ],
        "a book the policy refuses View is left out"
    );
    assert_eq!(
        results(&groups, "Shelf"),
        [("Dune".to_string(), format!("/admin/shelf/{}/edit", dune.id))],
        "without a detail page a result needs Update too"
    );
}

#[tokio::test]
async fn a_field_the_request_may_not_view_neither_matches_nor_shows() {
    let db = db().await;
    book(&db, "Dune", "978-0441013593", "zebra").await;
    let router = mount(db.clone(), books_panel()).expect("the panel mounts");

    assert!(search(&router, "zebra").await.is_empty());
    let groups = search(&router, "978-0441").await;
    assert_eq!(
        groups[0].results[0].details,
        [("ISBN".to_string(), "978-0441013593".to_string())]
    );
}

/// The integration panel signs no one in: the panel declares the shard, so it answers for this
/// panel, and lists the page's results.
#[tokio::test]
async fn the_results_shard_answers_for_a_panel_without_sign_in() {
    let db = db().await;
    let dune = book(&db, "Dune", "1", "").await;
    let router = mount(db.clone(), books_panel()).expect("the panel mounts");

    let response = TestClient::new(&router)
        .call_procedure(RESULTS_SHARD, r#"{"args":["/admin","dune"],"signals":{}}"#)
        .await;

    assert_eq!(response.status(), 200);
    let groups = search_results(&body_string(response).await);
    assert_eq!(
        results(&groups, "Books"),
        [("Dune".to_string(), format!("/admin/books/{}", dune.id))]
    );
}

/// Records the policy refuses fill more than the box's limit ahead of the one it allows, which
/// still shows.
#[tokio::test]
async fn records_the_policy_refuses_hide_no_record_it_allows() {
    let db = db().await;
    for n in 0..30 {
        book(&db, &format!("Hidden Dune {n:02}"), "1", "").await;
    }
    let zed = book(&db, "Zed Dune", "2", "").await;
    let router = mount(db.clone(), books_panel()).expect("the panel mounts");

    let response = TestClient::new(&router)
        .call_procedure(RESULTS_SHARD, r#"{"args":["/admin","dune"],"signals":{}}"#)
        .await;
    let groups = search_results(&body_string(response).await);
    assert_eq!(
        results(&groups, "Books"),
        [("Zed Dune".to_string(), format!("/admin/books/{}", zed.id))]
    );
}

/// The shard runs as the first panel that declares it, so on the second it lists no results, and
/// links that panel's results page, which does.
#[tokio::test]
async fn on_a_second_panel_without_sign_in_the_box_links_its_results_page() {
    let db = db().await;
    book(&db, "Dune", "1", "").await;
    let router = Router::builder()
        .discover()
        .with(db.clone())
        .panel(books_panel())
        .and_then(|router| router.panel(books_panel_at("staff")))
        .expect("the panels mount")
        .build();

    let response = TestClient::new(&router)
        .call_procedure(RESULTS_SHARD, r#"{"args":["/staff","dune"],"signals":{}}"#)
        .await;
    let html = body_string(response).await;
    assert!(search_results(&html).is_empty());
    assert!(html.contains(r#"href="/staff/-/search?q=dune""#), "{html}");
    let page = TestClient::new(&router).get("/staff/-/search?q=dune").await;
    assert_eq!(
        results(&search_results(&body_string(page).await), "Books").len(),
        1
    );
}

/// The shard takes its prefix from the client, so a prefix no panel mounts renders no link.
#[tokio::test]
async fn the_box_links_no_prefix_the_router_does_not_mount() {
    let db = db().await;
    let router = mount(db.clone(), books_panel()).expect("the panel mounts");

    for prefix in ["//evil.example", "https://evil.example", "/staff"] {
        let args = format!(r#"{{"args":["{prefix}","dune"],"signals":{{}}}}"#);
        let response = TestClient::new(&router)
            .call_procedure(RESULTS_SHARD, &args)
            .await;
        let html = body_string(response).await;
        assert!(!html.contains("data-search-all"), "{prefix}: {html}");
    }
}

/// A panel opted out, and one whose every resource is, show no box and serve no results page.
#[tokio::test]
async fn a_panel_without_global_search_shows_no_box_and_serves_no_results_page() {
    let db = db().await;
    book(&db, "Dune", "1", "").await;
    let searching = mount(db.clone(), books_panel()).expect("the panel mounts");
    let html = body_string(TestClient::new(&searching).get("/admin/books").await).await;
    assert_eq!(search_box(&html).as_deref(), Some("/admin/-/search"));

    for panel in [
        books_panel().without_global_search(),
        panel().resource::<ArchiveResource>(),
    ] {
        let router = mount(db.clone(), panel).expect("the panel mounts");
        let client = TestClient::new(&router);
        let html = body_string(client.get("/admin/archive").await).await;
        assert!(search_box(&html).is_none());
        assert_eq!(client.get("/admin/-/search?q=dune").await.status(), 404);
    }
}
