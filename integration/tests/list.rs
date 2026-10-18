//! A resource's list page over test-local resources: the row chrome each policy and form allows,
//! and the signals that carry each list's query.

use std::collections::HashSet;

use tablo::{Ability, NoForm, Panel, Resource, ResourceDef, Table, TextColumn, lens};
use toasty::Db;
use topcoat::{
    context::Cx,
    router::{Body, Router, Slot},
    view::{BoxView, ViewExt},
};
use uuid::Uuid;

use crate::common::{body_string, get, memory_db, mount, panel, rows};

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
    Table::new(TextColumn::new(lens!(Item.name)).searchable()).paginate(25)
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

async fn items(names: &[&str]) -> Db {
    let mut db = memory_db(toasty::models!(Item)).await;
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
        assert!(!html.contains("Delete selected"), "{html}");
    }
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
    let declaration = html
        .split("::topcoat::signal(")
        .skip(1)
        .find(|declaration| declaration.contains("&quot;v&quot;:&quot;q=Bob&quot;"))
        .unwrap_or_else(|| panic!("the page declares its query signal: {html}"));
    let id = declaration
        .split("&quot;id&quot;:&quot;")
        .nth(1)
        .and_then(|rest| rest.split("&quot;").next())
        .expect("a signal id");

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
