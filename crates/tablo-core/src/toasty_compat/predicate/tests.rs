use toasty::{Deferred, schema::Model as _, stmt::List};

use super::root_fields;

#[derive(Debug, toasty::Model)]
struct Shelf {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
    open: bool,
    #[has_many]
    books: Deferred<Vec<Book>>,
}

#[derive(Debug, toasty::Model)]
struct Book {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    #[index]
    shelf_id: uuid::Uuid,
    #[belongs_to(key = shelf_id, references = id)]
    shelf: Deferred<Shelf>,
}

/// A predicate over the row's own fields reads each field it compares, once.
#[test]
fn a_predicate_reads_the_fields_it_compares() {
    let fields = Shelf::fields();
    let predicate = fields
        .name()
        .eq("A")
        .and(fields.open().eq(true))
        .or(fields.name().eq("B"));
    assert_eq!(
        root_fields::<Shelf>(&predicate),
        (vec!["name".to_string(), "open".to_string()], false)
    );
}

/// A predicate through a relation reads the relation field it starts at, not the related
/// model's fields its subquery compares.
#[test]
fn a_relation_predicate_reads_the_relation() {
    let any = Shelf::fields()
        .books()
        .any(Book::fields().title().eq("Dune"));
    assert_eq!(
        root_fields::<Shelf>(&any),
        (vec!["books".to_string()], false)
    );
    let through = Book::fields().shelf().name().eq("A");
    assert_eq!(
        root_fields::<Book>(&through),
        (vec!["shelf".to_string()], false)
    );
}

/// A predicate over the whole record names no field, so it reads as unresolved.
#[test]
fn a_whole_record_predicate_is_unresolved() {
    let predicate = Book::path_root().in_query(
        toasty::stmt::Query::<List<Book>>::all().filter(Book::fields().title().eq("Dune")),
    );
    assert!(root_fields::<Book>(&predicate).1);
}
