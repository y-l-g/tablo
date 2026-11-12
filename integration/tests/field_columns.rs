//! A model's derived `columns()`: each field placed as it is reads as the default table and
//! detail page show it, and a kind method keeps the field's `label` key.

use tablo::{
    Detail, NoForm, Resource, ResourceDef, Table,
    testing::{columns, detail_entries, rows, sortable_columns},
};
use toasty::Db;
use uuid::Uuid;

use crate::common::{body_string, get, panel_router, test_db};

#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo::Options)]
enum Priority {
    Low,
    High,
}

/// A stage note, an embedded value the table leaves out.
#[derive(Debug, Clone, toasty::Embed, tablo::EmbeddedForm)]
struct Note {
    text: String,
}

#[derive(Debug, Clone, toasty::Model, tablo::PanelModel)]
struct Chore {
    #[key]
    #[auto]
    id: Uuid,
    title: String,
    priority: Priority,
    done: bool,
    #[columns(label = "Minutes")]
    estimate: i64,
    due_at: jiff::Timestamp,
    note: Note,
}

/// The model's default table and detail page.
struct DefaultChores;

impl Resource for DefaultChores {
    type Model = Chore;
    type Form = NoForm<Chore>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("default-chores")
            .policy(tablo::ReadOnly)
    }
}

/// Every field of `columns()` placed as it is, in the table and on the detail page.
struct PlacedChores;

impl Resource for PlacedChores {
    type Model = Chore;
    type Form = NoForm<Chore>;

    fn declare() -> ResourceDef<Self> {
        let table = Chore::columns();
        let detail = Chore::columns();
        ResourceDef::new()
            .slug("placed-chores")
            .policy(tablo::ReadOnly)
            .table(Table::new((
                table.title,
                table.priority,
                table.done,
                table.estimate,
                table.due_at,
                table.note,
            )))
            .detail(Detail::new((
                detail.title,
                detail.priority,
                detail.done,
                detail.estimate,
                detail.due_at,
                detail.note,
            )))
    }
}

/// A kind method in place of the derive's pick.
struct KindChores;

impl Resource for KindChores {
    type Model = Chore;
    type Form = NoForm<Chore>;

    fn declare() -> ResourceDef<Self> {
        let col = Chore::columns();
        ResourceDef::new()
            .slug("kind-chores")
            .policy(tablo::ReadOnly)
            .table(Table::new((col.title.text(), col.estimate.text())))
    }
}

async fn chore_db() -> (Db, Chore) {
    let mut db = test_db(toasty::models!(Chore)).await;
    let chore = toasty::create!(Chore {
        title: "Water the plants".to_string(),
        priority: Priority::High,
        done: false,
        estimate: 5,
        due_at: "2026-03-01T09:30:00Z".parse::<jiff::Timestamp>().unwrap(),
        note: Note {
            text: "Mind the fern".to_string(),
        },
    })
    .exec(&mut db)
    .await
    .expect("seed chore");
    toasty::create!(Chore {
        title: "Take out the bins".to_string(),
        priority: Priority::Low,
        done: true,
        estimate: 2,
        due_at: "2026-03-02T18:00:00Z".parse::<jiff::Timestamp>().unwrap(),
        note: Note {
            text: "Blue lid".to_string(),
        },
    })
    .exec(&mut db)
    .await
    .expect("seed chore");
    (db, chore)
}

/// Placed as it is, each field lists, sorts and searches as the default table does, and an
/// embedded value adds no table column.
#[tokio::test]
async fn placed_fields_list_as_the_default_table_does() {
    let (db, _) = chore_db().await;
    let defaults = panel_router::<DefaultChores>(db.clone());
    let placed = panel_router::<PlacedChores>(db);

    let expected = body_string(get(&defaults, "/admin/default-chores").await).await;
    let actual = body_string(get(&placed, "/admin/placed-chores").await).await;
    assert_eq!(columns(&actual), columns(&expected));
    assert_eq!(sortable_columns(&actual), sortable_columns(&expected));
    let cells = |html: &str| {
        rows(html)
            .into_iter()
            .map(|row| row.cells)
            .collect::<Vec<_>>()
    };
    assert_eq!(cells(&actual), cells(&expected));
    assert!(!columns(&actual).contains(&"note".to_string()));

    let searched = body_string(get(&placed, "/admin/placed-chores?q=plants").await).await;
    assert_eq!(rows(&searched).len(), 1, "the title searches: {searched}");
}

/// Placed on a detail page, each field shows as the default detail page does: an embedded value
/// leaf by leaf, which the table left out.
#[tokio::test]
async fn placed_fields_show_as_the_default_detail_page_does() {
    let (db, chore) = chore_db().await;
    let defaults = panel_router::<DefaultChores>(db.clone());
    let placed = panel_router::<PlacedChores>(db);

    let expected =
        body_string(get(&defaults, &format!("/admin/default-chores/{}", chore.id)).await).await;
    let actual =
        body_string(get(&placed, &format!("/admin/placed-chores/{}", chore.id)).await).await;
    assert_eq!(detail_entries(&actual), detail_entries(&expected));
    assert!(
        detail_entries(&actual)
            .iter()
            .any(|entry| entry.text.contains("Mind the fern")),
        "{actual}"
    );
}

/// A kind method builds that column type plain, without the derive's sort, and labelled by the
/// field's `label` key.
#[tokio::test]
async fn a_kind_is_the_plain_column_with_the_label_key() {
    let (db, _) = chore_db().await;
    let router = panel_router::<KindChores>(db);

    let html = body_string(get(&router, "/admin/kind-chores").await).await;
    assert_eq!(columns(&html), ["title", "estimate"]);
    assert!(sortable_columns(&html).is_empty(), "{html}");
    assert!(html.contains(">Minutes<"), "the label key names it: {html}");
}
