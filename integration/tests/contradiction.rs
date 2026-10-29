//! A tab and a filter no row satisfies together, on a table that loads a relation: the list lists
//! nothing rather than fail, under each pagination. Toasty folds their filter to `false` and
//! panics planning the relation's include over it, which would take the database connection down
//! with the request.
use tablo::{
    ListTab, NoForm, Pagination, QueryFilter, RelationColumn, Resource, ResourceDef, Table,
    TextColumn, lens, relation,
    testing::{EmptyReason, empty_table},
};
use toasty::Db;
use uuid::Uuid;

use crate::common::{body_string, get, mount, panel, test_db};

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Shelf {
    #[key]
    #[auto]
    id: Uuid,
    name: String,
    #[has_many]
    boxes: toasty::Deferred<Vec<Crate>>,
}

#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
struct Crate {
    #[key]
    #[auto]
    id: Uuid,
    #[index]
    shelf_id: Uuid,
    #[belongs_to(key = shelf_id, references = id)]
    shelf: toasty::Deferred<Shelf>,
    label: String,
}

struct Shelves;

impl Resource for Shelves {
    type Model = Shelf;
    type Form = NoForm<Shelf>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .policy(tablo::Allow)
            .table(Table::new(TextColumn::new(lens!(Shelf.name))))
    }
}

/// The crates, with a tab and a filter that each ask the label for one value.
fn crates_table() -> Table<Crate> {
    Table::new((
        TextColumn::new(lens!(Crate.label)),
        RelationColumn::of::<Shelves>(relation!(Crate.shelf)),
    ))
    .tab(ListTab::new("all", "All"))
    .tab(ListTab::new("red", "Red").query(Crate::fields().label().eq("red")))
    .filters(QueryFilter::new("blue", "Blue").option("Blue", Crate::fields().label().eq("blue")))
}

struct SimpleCrates;

impl Resource for SimpleCrates {
    type Model = Crate;
    type Form = NoForm<Crate>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("crates")
            .policy(tablo::Allow)
            .table(crates_table().pagination(Pagination::Simple))
    }
}

struct CursoredCrates;

impl Resource for CursoredCrates {
    type Model = Crate;
    type Form = NoForm<Crate>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("crates")
            .policy(tablo::Allow)
            .table(crates_table().pagination(Pagination::Cursor))
    }
}

/// One shelf holding a red and a blue crate.
async fn stocked() -> Db {
    let mut db = test_db(toasty::models!(Shelf, Crate)).await;
    let shelf = toasty::create!(Shelf {
        name: "Top".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    for label in ["red", "blue"] {
        toasty::create!(Crate {
            shelf_id: shelf.id,
            label: label.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    db
}

const CONTRADICTION: &str = "tab=red&f.blue=Blue";

/// Under each pagination the list shows the filters' empty state, and lists again afterwards.
#[tokio::test]
async fn a_contradiction_lists_nothing_under_each_pagination() {
    for (name, router) in [
        (
            "simple",
            mount(
                stocked().await,
                panel().resource::<Shelves>().resource::<SimpleCrates>(),
            ),
        ),
        (
            "cursor",
            mount(
                stocked().await,
                panel().resource::<Shelves>().resource::<CursoredCrates>(),
            ),
        ),
    ] {
        let router = router.expect("panel builds");
        let html = body_string(get(&router, &format!("/admin/crates?{CONTRADICTION}")).await).await;
        assert_eq!(
            empty_table(&html).map(|empty| empty.reason),
            Some(EmptyReason::Filters),
            "{name}: {html}"
        );
        let after = get(&router, "/admin/crates").await;
        assert_eq!(after.status(), 200, "{name}");
    }
}
