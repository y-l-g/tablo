use super::*;
use crate::{ComputedColumn, lens, test_support::User};

/// `%` and `_` in a search term are literal characters, not
/// wildcards, and the term is wrapped for a substring match.
#[test]
fn search_pattern_escapes_like_metacharacters() {
    assert_eq!(escape_like_pattern("Ada"), "%Ada%");
    assert_eq!(escape_like_pattern("100%"), "%100\\%%");
    assert_eq!(escape_like_pattern("a_b"), "%a\\_b%");
    assert_eq!(escape_like_pattern("back\\slash"), "%back\\\\slash%");
}

#[test]
fn text_column_renders_cells_via_typed_projection() {
    let plain = TextColumn::new(lens!(User.name));
    let decorated = TextColumn::new(lens!(User.name)).format(|name| format!("{name}!"));
    let row = User {
        id: uuid::Uuid::nil(),
        name: "Ada".to_string(),
    };
    assert_eq!(plain.text(&crate::test_support::cx(), &row), "Ada");
    assert_eq!(decorated.text(&crate::test_support::cx(), &row), "Ada!");
    assert_eq!(plain.name(), "name");
    assert_eq!(Column::label(&plain), "Name");
    let relabelled = TextColumn::new(lens!(User.name)).label("Full name");
    assert_eq!(Column::label(&relabelled), "Full name");
    assert_eq!(relabelled.name(), "name", "a label renames no column");
}

/// A column's includes accumulate across calls and keep each relation once, so
/// the table's union loads every relation the columns read, once.
#[test]
fn text_column_includes_accumulate_once_per_relation() {
    #[derive(Debug, toasty::Model, Clone)]
    struct Owner {
        #[key]
        #[auto]
        id: uuid::Uuid,
    }

    #[derive(Debug, toasty::Model, Clone)]
    struct Pet {
        #[key]
        #[auto]
        id: uuid::Uuid,
        #[index]
        owner_id: uuid::Uuid,
        #[belongs_to(key = owner_id, references = id)]
        owner: toasty::Deferred<Owner>,
        #[index]
        vet_id: uuid::Uuid,
        #[belongs_to(key = vet_id, references = id)]
        vet: toasty::Deferred<Owner>,
    }

    let column = ComputedColumn::new("Owner", |p: &Pet| p.id.to_string())
        .include(Pet::fields().owner())
        .include(Pet::fields().vet())
        .include(Pet::fields().owner());
    assert_eq!(
        column.includes().len(),
        2,
        "two relations, one repeated: the column keeps each once"
    );
}
