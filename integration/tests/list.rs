//! A resource's list page over test-local resources: the row chrome each policy and form allows,
//! the row links it may carry, the signals that carry each list's query, and the list query its
//! widgets read.

use std::collections::HashSet;

use tablo::{
    Ability, NoForm, Page, PageDef, Panel, Resource, ResourceDef, RowLink, Stat, Table, TextColumn,
    Widget, lens, testing::stats,
};
use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    router::{Body, Router, Slot},
    view::{BoxView, View, ViewExt},
};
use uuid::Uuid;

use crate::common::{body_string, confirms_first, get, mount, panel, rows, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Item {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

#[derive(tablo::RecordForm)]
#[form(model = Item)]
struct ItemForm {
    name: String,
}

fn items_table() -> Table<Item> {
    Table::new(TextColumn::new(lens!(Item.name)).searchable()).page_size(25)
}

fn list_only(_cx: &Cx, ability: Ability<'_, Item>) -> bool {
    matches!(ability, Ability::ViewAny)
}

fn editable(_cx: &Cx, ability: Ability<'_, Item>) -> bool {
    matches!(
        ability,
        Ability::ViewAny | Ability::View(_) | Ability::Create | Ability::Update(_)
    )
}

/// Lists the items, and allows nothing else.
struct ReadOnlyResource;

impl Resource for ReadOnlyResource {
    type Model = Item;
    type Form = NoForm<Item>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("items")
            .policy(list_only)
            .table(items_table())
    }
}

/// A form, and a policy letting the reader update every row.
struct EditableResource;

impl Resource for EditableResource {
    type Model = Item;
    type Form = ItemForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("items")
            .policy(editable)
            .table(items_table())
    }
}

/// A form no one may update with.
struct DeniedResource;

impl Resource for DeniedResource {
    type Model = Item;
    type Form = ItemForm;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("items")
            .policy(list_only)
            .table(items_table())
    }
}

/// The same list under another slug.
struct OtherResource;

impl Resource for OtherResource {
    type Model = Item;
    type Form = NoForm<Item>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("others")
            .policy(list_only)
            .table(items_table())
    }
}

/// The items, with a widget above their list counting the ones it shows.
struct CountedResource;

impl Resource for CountedResource {
    type Model = Item;
    type Form = NoForm<Item>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("items")
            .policy(list_only)
            .table(items_table())
            .list_widget::<Listed<CountedResource>>()
    }
}

/// The items, each linking the URL its name spells.
struct LinkedResource;

impl Resource for LinkedResource {
    type Model = Item;
    type Form = NoForm<Item>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().slug("items").policy(list_only).table(
            items_table().row_link(RowLink::new("Open", |item: &Item| Some(item.name.clone()))),
        )
    }
}

/// The items no one may list.
struct HiddenResource;

impl Resource for HiddenResource {
    type Model = Item;
    type Form = NoForm<Item>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("hidden")
            .policy(|_cx: &Cx, _ability: Ability<'_, Item>| false)
            .table(items_table())
    }
}

/// Counts the records `R`'s list shows.
struct Listed<R>(std::marker::PhantomData<R>);

impl<R: Resource<Model = Item>> Widget for Listed<R> {
    async fn render(cx: &Cx) -> Result<impl View> {
        let count = tablo::db::count(cx, tablo::list_query::<R>(cx).await?).await?;
        Ok(tablo::stats(
            cx,
            vec![Stat::new("Listed", count.to_string())],
        ))
    }
}

/// A page counting `R`'s list.
struct Counts<R>(std::marker::PhantomData<R>);

impl<R: Resource<Model = Item>> Page for Counts<R> {
    fn declare() -> PageDef {
        PageDef::new().slug("counts").widget::<Listed<R>>()
    }
}

async fn items(names: &[&str]) -> Db {
    let mut db = test_db(toasty::models!(Item)).await;
    for name in names {
        toasty::create!(Item {
            name: name.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    db
}

async fn list_page<R: Resource>() -> String {
    let router = mount(items(&["Ada"]).await, panel().resource::<R>()).expect("panel builds");
    let response = get(&router, "/admin/items").await;
    assert!(response.status().is_success());
    body_string(response).await
}

/// A row links its edit form only when the resource has one and the policy lets the reader
/// update the row; a list allowing nothing else renders no delete or bulk chrome.
#[tokio::test]
async fn a_row_offers_only_the_writes_the_resource_allows() {
    let html = list_page::<EditableResource>().await;
    let row = &rows(&html)[0];
    assert!(
        row.actions
            .edit
            .as_deref()
            .is_some_and(|href| href.ends_with("/edit")),
        "{html}"
    );
    // The row and Create links navigate through the runtime, and the panel turns prefetching off.
    assert!(
        html.contains("data-topcoat-link=\"never\"")
            && !html.contains("data-topcoat-link=\"intent\""),
        "{html}"
    );

    for html in [
        list_page::<ReadOnlyResource>().await,
        list_page::<DeniedResource>().await,
    ] {
        let rows = rows(&html);
        assert_eq!(rows.len(), 1, "{html}");
        assert_eq!(rows[0].actions.edit, None, "{html}");
        assert_eq!(rows[0].actions.delete_action, None, "{html}");
        assert_eq!(rows[0].select_value, None, "no bulk selection: {html}");
        assert_eq!(
            confirms_first(&html, "/admin/items/-/bulk-delete"),
            None,
            "no bulk delete: {html}"
        );
    }
}

/// The id of the signal `html` declares holding the list query `query`.
fn query_signal(html: &str, query: &str) -> String {
    let declaration = html
        .split("::topcoat::signal(")
        .skip(1)
        .find(|declaration| declaration.contains(&format!("&quot;v&quot;:&quot;{query}&quot;")))
        .unwrap_or_else(|| panic!("the page declares its query signal: {html}"));
    declaration
        .split("&quot;id&quot;:&quot;")
        .nth(1)
        .and_then(|rest| rest.split("&quot;").next())
        .expect("a signal id")
        .to_string()
}

/// The ids of the signals `html` declares.
fn signal_ids(html: &str) -> HashSet<String> {
    html.split("&quot;id&quot;:&quot;")
        .skip(1)
        .filter_map(|rest| rest.split("&quot;").next().map(str::to_string))
        .collect()
}

/// Two lists declare distinct signals, so a rerun of one never reads the other's state.
#[tokio::test]
async fn two_lists_declare_distinct_signals() {
    // The shell's sidebar signals match on every page; a bare layout leaves only the tables'.
    fn bare<'a>(_cx: &'a Cx, slot: Slot<'a>) -> BoxView<'a> {
        slot.boxed()
    }
    let router = mount(
        items(&[]).await,
        Panel::new("admin")
            .auth(tablo::Auth::disabled())
            .layout(bare)
            .resource::<ReadOnlyResource>()
            .resource::<OtherResource>(),
    )
    .expect("panel builds");
    let items = signal_ids(&body_string(get(&router, "/admin/items").await).await);
    let others = signal_ids(&body_string(get(&router, "/admin/others").await).await);
    assert!(!items.is_empty() && !others.is_empty());
    assert!(
        items.is_disjoint(&others),
        "{:?}",
        items.intersection(&others)
    );
}

/// A list reads its state from its query signal: a rerun carrying another query renders that
/// query's rows, whatever the URL says.
#[tokio::test]
async fn a_rerun_renders_the_query_its_signal_carries() {
    let router: Router = mount(
        items(&["Ada", "Bob"]).await,
        panel().resource::<ReadOnlyResource>(),
    )
    .expect("panel builds");
    let names = |html: &str| -> Vec<String> {
        rows(html)
            .into_iter()
            .map(|row| row.cells[0].clone())
            .collect()
    };

    let html = body_string(get(&router, "/admin/items?q=Bob").await).await;
    assert_eq!(names(&html), ["Bob"], "{html}");
    let id = query_signal(&html, "q=Bob");

    let rerun = http::Request::builder()
        .method(http::Method::POST)
        .uri("/admin/items?q=Bob")
        .header(&topcoat::runtime::RUNTIME_HEADER, "true")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"signals":{{"{id}":"q=Ada"}}}}"#)))
        .unwrap();
    let rerun = body_string(router.handle(rerun).await).await;
    assert_eq!(names(&rerun), ["Ada"], "{rerun}");
}

/// A runtime navigation to the list under another query, a stat tile's or a sidebar link's,
/// renders the URL's rows, not the query the page it leaves carries in its signal.
#[tokio::test]
async fn a_navigation_to_another_query_renders_the_url() {
    let router: Router = mount(
        items(&["Ada", "Bob"]).await,
        panel().resource::<ReadOnlyResource>(),
    )
    .expect("panel builds");
    let html = body_string(get(&router, "/admin/items?q=Bob").await).await;
    let id = query_signal(&html, "q=Bob");

    let navigation = http::Request::builder()
        .method(http::Method::POST)
        .uri("/admin/items?q=Ada")
        .header(&topcoat::runtime::RUNTIME_HEADER, "true")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"signals":{{"{id}":"q=Bob"}}}}"#)))
        .unwrap();
    let page = body_string(router.handle(navigation).await).await;
    let names: Vec<_> = rows(&page)
        .into_iter()
        .map(|row| row.cells[0].clone())
        .collect();
    assert_eq!(names, ["Ada"], "{page}");
}

/// The figures a list's widget reads follow the state the list shows: its search from the URL,
/// and on a rerun the query its signal carries.
#[tokio::test]
async fn a_list_widget_counts_what_the_list_shows() {
    let router: Router = mount(
        items(&["Ada", "Adam", "Bob"]).await,
        panel().resource::<CountedResource>(),
    )
    .expect("panel builds");
    let listed = |html: &str| {
        stats(html)
            .into_iter()
            .map(|tile| tile.value)
            .collect::<Vec<_>>()
    };

    let html = body_string(get(&router, "/admin/items?q=Bob").await).await;
    assert_eq!(listed(&html), ["1"], "{html}");
    let id = query_signal(&html, "q=Bob");
    let rerun = http::Request::builder()
        .method(http::Method::POST)
        .uri("/admin/items?q=Bob")
        .header(&topcoat::runtime::RUNTIME_HEADER, "true")
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(r#"{{"signals":{{"{id}":"q=Ad"}}}}"#)))
        .unwrap();
    let rerun = body_string(router.handle(rerun).await).await;
    assert_eq!(listed(&rerun), ["2"], "{rerun}");
}

/// Outside the list, the list query reads the whole list; it refuses a request that may not open
/// the list, failing the page.
#[tokio::test]
async fn a_list_query_reads_the_whole_list_elsewhere_and_refuses_who_may_not_list() {
    let router: Router = mount(
        items(&["Ada", "Bob"]).await,
        panel()
            .resource::<CountedResource>()
            .resource::<HiddenResource>()
            .page::<Counts<CountedResource>>(),
    )
    .expect("panel builds");
    let html = body_string(get(&router, "/admin/counts?q=Bob").await).await;
    assert_eq!(
        stats(&html)
            .into_iter()
            .map(|tile| tile.value)
            .collect::<Vec<_>>(),
        ["2"],
        "{html}"
    );

    let router: Router = mount(
        items(&["Ada"]).await,
        panel()
            .resource::<HiddenResource>()
            .page::<Counts<HiddenResource>>(),
    )
    .expect("panel builds");
    assert_eq!(get(&router, "/admin/counts").await.status(), 403);
}

/// A row link shows on a list allowing no other action, and only to a relative URL or one over a
/// scheme a cell may link: a `javascript:` URL, however spelled, shows no link.
#[tokio::test]
async fn a_row_links_only_a_url_a_cell_may_link() {
    let names = [
        "/docs/a",
        "https://example.com/a",
        "javascript:alert(1)",
        " JavaScript:alert(2)",
    ];
    let router = mount(items(&names).await, panel().resource::<LinkedResource>()).expect("builds");
    let html = body_string(get(&router, "/admin/items").await).await;
    let mut links: Vec<String> = rows(&html)
        .into_iter()
        .flat_map(|row| row.actions.links)
        .collect();
    links.sort();
    assert_eq!(links, ["/docs/a", "https://example.com/a"], "{html}");
}
