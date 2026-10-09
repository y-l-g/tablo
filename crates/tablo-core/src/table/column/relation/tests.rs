use toasty::Db;

use super::*;
use crate::{Detail, policy::Ability, test_support::memory_db};

#[derive(Debug, Clone, toasty::Model)]
struct Shelf {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
    #[has_many]
    books: Deferred<Vec<Book>>,
}

#[derive(Debug, Clone, toasty::Model)]
struct Reader {
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
    #[index]
    shelf_id: uuid::Uuid,
    #[belongs_to(key = shelf_id, references = id)]
    shelf: Deferred<Shelf>,
    #[index]
    borrower_id: Option<uuid::Uuid>,
    #[belongs_to(key = borrower_id, references = id)]
    borrower: Deferred<Option<Reader>>,
}

fn shelf_name() -> RelationColumn<Book> {
    RelationColumn::new(relation!(Book.shelf), |s: &Shelf| s.name.clone())
}

fn borrower_name() -> RelationColumn<Book> {
    RelationColumn::new(relation!(Book.borrower), |r: &Reader| r.name.clone())
}

async fn library() -> Db {
    let mut db = memory_db(toasty::models!(Shelf, Reader, Book)).await;
    let shelf = toasty::create!(Shelf {
        name: "Sci-fi".to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();
    let reader = toasty::create!(Reader {
        name: "Ada".to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();
    for (title, borrower_id) in [("Dune", Some(reader.id)), ("Solaris", None)] {
        toasty::create!(Book {
            title: title.to_string(),
            shelf_id: shelf.id,
            borrower_id,
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    db
}

/// A relation column is headed by its field and names it, and declares the include of that field.
#[test]
fn a_relation_column_names_its_field_and_declares_its_include() {
    let column = shelf_name();
    assert_eq!(Column::<Book>::name(&column), "shelf");
    assert_eq!(Column::<Book>::label(&column), "Shelf");
    assert_eq!(column.column_width(), ColumnWidth::Wide);
    assert_eq!(
        column.includes().into_vec(),
        Includes::new().with(Book::fields().shelf()).into_vec()
    );
    assert!(column.misdeclared().is_none());

    let count = CountColumn::new(relation!(Shelf.books));
    assert_eq!(Column::<Shelf>::name(&count), "books");
    assert_eq!(Column::<Shelf>::label(&count), "Books");
    assert_eq!(count.column_width(), ColumnWidth::Narrow);
    assert_eq!(
        count.includes().into_vec(),
        Includes::new().with(Shelf::fields().books()).into_vec()
    );
}

/// The shelves a book is shelved on, labelled by name with a prefix the test can tell apart.
struct Shelves;

impl crate::schema::OptionSource for Shelves {
    type Model = Shelf;

    fn scoped_query(
        _cx: &topcoat::context::Cx,
    ) -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Shelf>>> {
        Ok(toasty::stmt::Query::all())
    }

    fn label(_cx: &topcoat::context::Cx, shelf: &Shelf) -> String {
        format!("Shelf {}", shelf.name)
    }
}

/// A relation column of a source shows each related record by the source's label, the one its
/// relationship choices offer.
#[tokio::test]
async fn a_relation_column_of_a_source_shows_its_label() {
    let cx = crate::test_support::cx();
    let mut db = library().await;
    let column = RelationColumn::of::<Shelves>(relation!(Book.shelf));
    assert_eq!(Column::<Book>::name(&column), "shelf");
    let book = Detail::new(column.clone())
        .include_relations(toasty::stmt::Query::all())
        .first()
        .exec(&mut db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(column.text(&cx, &book), "Shelf Sci-fi");
}

/// A label renames the column, so one table shows two values of one relation.
#[test]
fn two_labelled_columns_over_one_relation_declare_one_table() {
    let table = crate::Table::new((
        shelf_name(),
        RelationColumn::new(relation!(Book.shelf), |s: &Shelf| s.id.to_string()).label("Shelf id"),
    ));
    assert_eq!(table.declaration_errors(), []);

    let twice = crate::Table::new((shelf_name(), shelf_name()));
    assert_eq!(
        twice.declaration_errors(),
        [DeclarationErrorKind::DuplicateColumn {
            name: "shelf".to_string()
        }]
    );
}

/// The page loads what the relation columns declare, so each row shows the related record's text,
/// an empty cell for a nullable relation holding none, and the count of a `has_many`.
#[tokio::test]
async fn the_page_loads_the_relations_its_relation_columns_read() {
    let cx = crate::test_support::cx();
    let mut db = library().await;

    let detail = Detail::new((shelf_name(), borrower_name()));
    let books = detail
        .include_relations(toasty::stmt::Query::all())
        .exec(&mut db)
        .await
        .unwrap();
    let mut rows: Vec<_> = books
        .iter()
        .map(|b| {
            (
                b.title.as_str(),
                shelf_name().text(&cx, b),
                borrower_name().text(&cx, b),
            )
        })
        .collect();
    rows.sort();
    assert_eq!(
        rows,
        [
            ("Dune", "Sci-fi".to_string(), "Ada".to_string()),
            ("Solaris", "Sci-fi".to_string(), String::new()),
        ]
    );

    let count = CountColumn::new(relation!(Shelf.books));
    let shelf = Detail::new(count.clone())
        .include_relations(toasty::stmt::Query::all())
        .first()
        .exec(&mut db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(count.text(&cx, &shelf), "2");

    let titles = RelationColumn::list::<Titles>(relation!(Shelf.books));
    let shelf = Detail::new(titles.clone())
        .include_relations(toasty::stmt::Query::all())
        .first()
        .exec(&mut db)
        .await
        .unwrap()
        .unwrap();
    let mut listed: Vec<String> = titles
        .text(&cx, &shelf)
        .split(", ")
        .map(str::to_string)
        .collect();
    listed.sort();
    assert_eq!(
        listed,
        ["Dune"],
        "each record the user may view, by the source's label"
    );
}

/// Books, labelled by their title; "Solaris" cannot be viewed.
struct Titles;

impl OptionSource for Titles {
    type Model = Book;

    fn scoped_query(_cx: &Cx) -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Book>>> {
        Ok(toasty::stmt::Query::all())
    }

    fn label(_cx: &Cx, book: &Book) -> String {
        book.title.clone()
    }

    fn allows(_cx: &Cx, ability: Ability<'_, Book>) -> bool {
        !matches!(ability, Ability::View(book) if book.title == "Solaris")
    }
}

/// A row loaded without the include its relation column declares is a loader's bug: a debug
/// build fails the assertion.
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "the `shelf` column declares its include")]
fn an_unloaded_relation_fails_a_debug_assertion() {
    let book = Book {
        id: uuid::Uuid::nil(),
        title: "Dune".to_string(),
        shelf_id: uuid::Uuid::nil(),
        shelf: Deferred::default(),
        borrower_id: None,
        borrower: Deferred::default(),
    };
    shelf_name().text(&crate::test_support::cx(), &book);
}
