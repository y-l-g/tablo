use super::*;
use crate::{
    lens,
    test_support::{cx, memory_db},
};

#[derive(Debug, Clone, toasty::Model)]
struct Shelf {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model)]
struct Book {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    lent: bool,
    #[index]
    shelf_id: uuid::Uuid,
    #[belongs_to(key = shelf_id, references = id)]
    shelf: toasty::Deferred<Shelf>,
}

fn book() -> Book {
    Book {
        id: uuid::Uuid::nil(),
        title: "Dune".to_string(),
        lent: true,
        shelf_id: uuid::Uuid::nil(),
        shelf: toasty::Deferred::default(),
    }
}

async fn render(detail: &Detail<Book>, record: &Book) -> String {
    let cx = cx();
    detail
        .render(&cx, record)
        .single()
        .await
        .unwrap()
        .render(&cx)
}

/// Each column shows its label over the value it reads off the typed record, inside the block
/// that holds it, and no control.
#[tokio::test]
async fn a_detail_shows_each_columns_value_in_its_block() {
    let detail = Detail::new((
        Section::new("Book").columns(Grid::new(2).columns((
            TextColumn::new(lens!(Book.title)),
            BooleanColumn::new(lens!(Book.lent)).labels("Lent", "On the shelf"),
        ))),
        ComputedColumn::new("Initial", |b: &Book| b.title[..1].to_string()),
    ));
    let html = render(&detail, &book()).await;
    let at = |needle: &str| {
        html.find(needle)
            .unwrap_or_else(|| panic!("no {needle} in {html}"))
    };
    assert!(
        at("Book") < at(">Title<") && at(">Title<") < at("Dune"),
        "{html}"
    );
    assert!(at(">Lent<") > at("Dune"), "the boolean's label: {html}");
    assert!(
        at(">Initial<") > at(">Lent<") && html.contains(">D<"),
        "{html}"
    );
    assert!(
        html.contains("grid-cols-2"),
        "the grid lays out its columns: {html}"
    );
    assert!(
        !html.contains("<input") && !html.contains("<select"),
        "{html}"
    );
}

/// The framework stores `""` rather than NULL, so a stored record cannot tell "no value" from an
/// empty one: both read as the dash.
#[tokio::test]
async fn an_empty_value_renders_as_a_dash() {
    let detail = Detail::new(TextColumn::new(lens!(Book.title)));
    let html = render(
        &detail,
        &Book {
            title: String::new(),
            ..book()
        },
    )
    .await;
    // The value node is the innermost `<div>` of the entry: located structurally rather than by
    // its utility classes, so a restyle cannot silently turn the lookup into an empty string.
    let start = html.rfind("<div").expect("the render has the value node");
    let open_end = html[start..].find('>').expect("its tag's end") + start + 1;
    let close = html[open_end..].find("</div>").expect("its closing tag") + open_end;
    assert!(
        html[open_end..close].contains('—'),
        "the value is the dash: {html}"
    );
    assert!(html.contains("Title"), "the label still renders: {html}");
}

/// A section holding no column renders its title alone.
#[tokio::test]
async fn an_empty_section_renders_its_title() {
    let detail = Detail::new((Section::new("Notes"), TextColumn::new(lens!(Book.title))));
    let html = render(&detail, &book()).await;
    assert!(html.contains("Notes") && html.contains("Dune"), "{html}");
}

/// The relations every column declares load with the record, however deep its block, so a column
/// reads a relation without a hand-written query.
#[tokio::test]
async fn a_detail_loads_the_relations_its_columns_declare() {
    let mut db = memory_db(toasty::models!(Shelf, Book)).await;
    let shelf = toasty::create!(Shelf {
        name: "Sci-fi".to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Book {
        title: "Dune".to_string(),
        lent: false,
        shelf_id: shelf.id,
    })
    .exec(&mut db)
    .await
    .unwrap();
    let shelf_name = || {
        ComputedColumn::new("Shelf", |b: &Book| b.shelf.get().name.clone())
            .include(Book::fields().shelf())
    };

    let bare = Detail::new(TextColumn::new(lens!(Book.title)));
    let record = bare
        .include_relations(toasty::stmt::Query::all())
        .first()
        .exec(&mut db)
        .await
        .unwrap()
        .unwrap();
    assert!(record.shelf.is_unloaded(), "no column reads the shelf");

    let detail = Detail::new((
        Section::new("Where").columns(Group::new().columns(shelf_name())),
        shelf_name(),
    ));
    let record = detail
        .include_relations(toasty::stmt::Query::all())
        .first()
        .exec(&mut db)
        .await
        .unwrap()
        .unwrap();
    assert!(!record.shelf.is_unloaded(), "the nested column's include");
    assert!(render(&detail, &record).await.contains("Sci-fi"));
}
