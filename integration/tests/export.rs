//! The CSV export: which rows it delivers, what it loads for them, and the 10,000-row cap
//! (`docs/guide/src/tables.md`) it refuses past rather than deliver part of the list.

use std::sync::atomic::{AtomicUsize, Ordering};

use tablo::{
    Ability, ComputedColumn, NoForm, ReadOnly, Resource, ResourceDef, Table, TextColumn, lens,
};
use toasty::Db;
use topcoat::{context::Cx, router::Router};
use uuid::Uuid;

use crate::common::{body_bytes, body_string, get, memory_db, panel_router};

/// The cap the guide documents.
const MAX_EXPORT_ROWS: usize = 10_000;

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

/// The items not named `denied…`.
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
    let mut db = memory_db(toasty::models!(Item)).await;
    if rows > 0 {
        let mut create = Item::create_many();
        for i in 0..rows {
            create = create.item(Item::create().name(name(i)));
        }
        create.exec(&mut db).await.unwrap();
    }
    db
}

/// Every second item is `denied…`.
fn interleaved(i: usize) -> String {
    if i.is_multiple_of(2) {
        format!("allowed-{i:05}")
    } else {
        format!("denied-{i:05}")
    }
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

#[tokio::test]
async fn an_export_holds_only_the_rows_the_reader_may_view() {
    let db = items(2, |i| ["allowed", "denied"][i].to_string()).await;
    let csv = export(
        &panel_router::<VisibleItemResource>(db),
        "/admin/items/-/export",
    )
    .await;
    assert!(csv.contains("allowed") && !csv.contains("denied"), "{csv}");
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
    let response = get(&panel_router::<ItemResource>(db), "/admin/items/-/export").await;
    assert_eq!(response.status(), http::StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!body_string(response).await.contains("item-"));
}

/// The cap counts the rows the reader may view, within a window of the cap's size: refused rows
/// interleaved in it leave the export whole, and rows beyond it refuse the export, though fewer
/// than the cap are viewable.
#[tokio::test]
async fn the_cap_counts_viewable_rows_within_its_window() {
    let db = items(MAX_EXPORT_ROWS + 1, interleaved).await;
    let csv = export(
        &panel_router::<VisibleItemResource>(db),
        "/admin/items/-/export",
    )
    .await;
    let rows: Vec<&str> = csv.lines().skip(1).collect();
    assert_eq!(rows.len(), (MAX_EXPORT_ROWS + 1).div_ceil(2));
    assert!(rows.iter().all(|row| row.starts_with("allowed-")), "{csv}");

    let db = items(MAX_EXPORT_ROWS + 1 + 20, interleaved).await;
    let response = get(
        &panel_router::<VisibleItemResource>(db),
        "/admin/items/-/export",
    )
    .await;
    assert_eq!(response.status(), http::StatusCode::PAYLOAD_TOO_LARGE);
    let body = body_string(response).await;
    assert!(
        !body.contains("allowed-") && !body.contains("denied-"),
        "{body:?}"
    );
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
            column.include(Volume::fields().shelf())
        } else {
            column
        };
        ResourceDef::new()
            .slug(if INCLUDES { "included" } else { "bare" })
            .policy(ReadOnly)
            .table(Table::new(column))
    }
}

static COUNTED_UNLOADED: AtomicUsize = AtomicUsize::new(0);
static STREAMED_LOADED: AtomicUsize = AtomicUsize::new(0);

/// Records whether each row `View` sees came with its shelf.
fn tally_shelves(_cx: &Cx, ability: Ability<'_, Volume>) -> bool {
    match ability {
        Ability::ViewAny => true,
        Ability::View(volume) => {
            let tally = if volume.shelf.is_unloaded() {
                &COUNTED_UNLOADED
            } else {
                &STREAMED_LOADED
            };
            tally.fetch_add(1, Ordering::SeqCst);
            true
        }
        _ => false,
    }
}

/// The included export, its `View` tallying the shelves.
struct TalliedVolumeResource;

impl Resource for TalliedVolumeResource {
    type Model = Volume;
    type Form = NoForm<Volume>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("tallied")
            .policy(tally_shelves)
            .table(Table::new(
                ComputedColumn::new("Shelf", shelf_name).include(Volume::fields().shelf()),
            ))
    }
}

async fn one_volume() -> Db {
    let mut db = memory_db(toasty::models!(Shelf, Volume)).await;
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

/// The pass counting the viewable rows loads no relation; the pass streaming them loads the
/// columns' includes.
#[tokio::test]
async fn only_the_streamed_rows_load_their_includes() {
    let router = panel_router::<TalliedVolumeResource>(one_volume().await);
    let csv = export(&router, "/admin/tallied/-/export").await;
    assert!(csv.contains("Sci-fi"), "{csv}");
    assert!(COUNTED_UNLOADED.load(Ordering::SeqCst) > 0);
    assert!(STREAMED_LOADED.load(Ordering::SeqCst) > 0);
}
