//! The CSV export: which rows it delivers, what it loads for them, and the row cap
//! ([`Limits::export_rows`](tablo::Limits::export_rows)) it refuses past rather than deliver part
//! of the list.

use tablo::{
    Ability, ComputedColumn, Limits, NoForm, ReadOnly, Resource, ResourceDef, Table, TextColumn,
    lens,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{body_bytes, body_string, get, mount, panel, panel_router, test_db};

/// The row cap the capped panel sets.
const MAX_EXPORT_ROWS: usize = 20;

/// `R` on a panel that exports at most [`MAX_EXPORT_ROWS`] rows.
fn capped_router<R: Resource>(db: Db) -> Router {
    let limits = Limits::default().export_rows(MAX_EXPORT_ROWS);
    mount(db, panel().limits(limits).resource::<R>()).expect("panel builds")
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Item {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

/// Every item.
struct ItemResource;

impl Resource for ItemResource {
    type Model = Item;
    type Form = NoForm<Item>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("items")
            .policy(ReadOnly)
            .table(Table::new(TextColumn::new(lens!(Item.name))))
    }
}

fn not_denied(_cx: &Cx, ability: Ability<'_, Item>) -> bool {
    match ability {
        Ability::ViewAny => true,
        Ability::View(item) => !item.name.starts_with("denied"),
        _ => false,
    }
}

/// Every item, opening none named `denied…`.
struct VisibleItemResource;

impl Resource for VisibleItemResource {
    type Model = Item;
    type Form = NoForm<Item>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("items")
            .policy(not_denied)
            .table(Table::new(TextColumn::new(lens!(Item.name))))
    }
}

async fn items(rows: usize, name: impl Fn(usize) -> String) -> Db {
    let mut db = test_db(toasty::models!(Item)).await;
    if rows > 0 {
        let mut create = Item::create_many();
        for i in 0..rows {
            create = create.item(Item::create().name(name(i)));
        }
        create.exec(&mut db).await.unwrap();
    }
    db
}

async fn export(router: &Router, uri: &str) -> String {
    let response = get(router, uri).await;
    assert!(
        response.status().is_success(),
        "{uri}: {}",
        response.status()
    );
    body_string(response).await
}

/// The export holds every row the list shows, the ones the policy refuses `View` included.
#[tokio::test]
async fn an_export_holds_the_rows_view_refuses() {
    let db = items(2, |i| ["allowed", "denied"][i].to_string()).await;
    let csv = export(
        &panel_router::<VisibleItemResource>(db),
        "/admin/items/-/export",
    )
    .await;
    assert!(csv.contains("allowed") && csv.contains("denied"), "{csv}");
}

/// The export streams without a length, and its chunks, 500 rows each, neither drop nor repeat a
/// row; the BOM variant holds the same rows.
#[tokio::test]
async fn an_export_streams_every_row_once() {
    let total = 2 * 500 + 203;
    let router = panel_router::<ItemResource>(items(total, |i| format!("item-{i:05}")).await);
    let response = get(&router, "/admin/items/-/export").await;
    assert!(response.status().is_success());
    assert!(
        response
            .headers()
            .get(http::header::CONTENT_LENGTH)
            .is_none()
    );
    assert_eq!(
        response.headers()[http::header::CONTENT_TYPE],
        "text/csv; charset=utf-8"
    );
    let csv = body_string(response).await;
    let mut lines = csv.lines();
    assert_eq!(lines.next(), Some("Name"));
    let mut names: Vec<&str> = lines.collect();
    names.sort_unstable();
    let expected: Vec<String> = (0..total).map(|i| format!("item-{i:05}")).collect();
    assert_eq!(names, expected);

    let bom = export(&router, "/admin/items/-/export?bom=1").await;
    assert_eq!(bom.strip_prefix('\u{FEFF}'), Some(csv.as_str()));
}

/// An empty table downloads the header, not a 0-byte body.
#[tokio::test]
async fn an_empty_export_is_the_header_line() {
    let router = panel_router::<ItemResource>(items(0, |_| String::new()).await);
    assert_eq!(
        body_bytes(get(&router, "/admin/items/-/export").await).await,
        b"Name\n"
    );
    assert_eq!(
        body_bytes(get(&router, "/admin/items/-/export?bom=1").await).await,
        "\u{FEFF}Name\n".as_bytes()
    );
}

/// One row past the cap answers 413 before any row streams.
#[tokio::test]
async fn an_export_past_the_cap_is_refused_whole() {
    let db = items(MAX_EXPORT_ROWS + 1, |i| format!("item-{i:05}")).await;
    let response = get(&capped_router::<ItemResource>(db), "/admin/items/-/export").await;
    assert_eq!(response.status(), http::StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!body_string(response).await.contains("item-"));
}

#[derive(Debug, Clone, toasty::Model)]
struct Shelf {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Volume {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    #[index]
    shelf_id: Uuid,
    #[belongs_to(key = shelf_id, references = id)]
    shelf: toasty::Deferred<Shelf>,
}

/// The shelf's name, or `(unloaded)` when the row came without it.
fn shelf_name(volume: &Volume) -> String {
    if volume.shelf.is_unloaded() {
        "(unloaded)".to_string()
    } else {
        volume.shelf.get().name.clone()
    }
}

/// Exports the shelf through a column including it when `INCLUDES`.
struct VolumeResource<const INCLUDES: bool>;

impl<const INCLUDES: bool> Resource for VolumeResource<INCLUDES> {
    type Model = Volume;
    type Form = NoForm<Volume>;

    fn declare() -> ResourceDef<Self> {
        let column = ComputedColumn::new("Shelf", shelf_name);
        let column = if INCLUDES {
            column.include_unscoped(Volume::fields().shelf())
        } else {
            column
        };
        ResourceDef::new()
            .slug(if INCLUDES { "included" } else { "bare" })
            .policy(ReadOnly)
            .table(Table::new(column))
    }
}

async fn one_volume() -> Db {
    let mut db = test_db(toasty::models!(Shelf, Volume)).await;
    let shelf = toasty::create!(Shelf {
        name: "Sci-fi".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Volume {
        title: "Dune".to_string(),
        shelf_id: shelf.id,
    })
    .exec(&mut db)
    .await
    .unwrap();
    db
}

/// The export loads the relations its columns include and no other.
#[tokio::test]
async fn an_export_loads_the_relations_its_columns_include() {
    let router = crate::common::mount(
        one_volume().await,
        crate::common::panel()
            .resource::<VolumeResource<true>>()
            .resource::<VolumeResource<false>>(),
    )
    .expect("panel builds");
    let included = export(&router, "/admin/included/-/export").await;
    assert!(
        included.contains("Sci-fi") && !included.contains("(unloaded)"),
        "{included}"
    );
    let bare = export(&router, "/admin/bare/-/export").await;
    assert!(bare.contains("(unloaded)"), "{bare}");
}
