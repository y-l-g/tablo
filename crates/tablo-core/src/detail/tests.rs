use super::*;
use crate::{ComputedColumn, TextColumn, lens, test_support::memory_db};

#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
struct Shelf {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
}

#[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
struct Book {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    #[index]
    shelf_id: uuid::Uuid,
    #[belongs_to(key = shelf_id, references = id)]
    shelf: toasty::Deferred<Shelf>,
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
    assert_eq!(record.shelf.get().name, "Sci-fi");
}
