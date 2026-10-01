use super::*;
use crate::test_support::User;

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
fn text_column_searchable_produces_a_substring_pattern() {
    let col = TextColumn::r#for(User::fields().name(), |u| u.name.clone()).searchable();
    assert!(
        col.search_expr("Ada").is_some(),
        "searchable should produce expr"
    );
    assert!(
        TextColumn::r#for(User::fields().name(), |u| u.name.clone())
            .search_expr("Ada")
            .is_none(),
        "non-searchable should be None"
    );
}

#[test]
fn text_column_sortable_produces_order_by() {
    let col = TextColumn::r#for(User::fields().name(), |u| u.name.clone()).sortable();
    assert!(
        col.order_by(false).is_some(),
        "sortable should produce order_by"
    );
    assert!(
        TextColumn::r#for(User::fields().name(), |u| u.name.clone())
            .order_by(false)
            .is_none(),
        "non-sortable should be None"
    );
}

#[test]
fn text_column_renders_cells_via_typed_projection() {
    let plain = TextColumn::r#for(User::fields().name(), |u| u.name.clone());
    let decorated = TextColumn::r#for(User::fields().name(), |u| format!("{}!", u.name));
    let row = User {
        id: uuid::Uuid::nil(),
        name: "Ada".to_string(),
    };
    assert_eq!(plain.text(&row), "Ada");
    assert_eq!(decorated.text(&row), "Ada!");
    assert_eq!(plain.name(), "name");
    assert_eq!(plain.label(), "Name");
}

#[test]
fn computed_searchable_is_misdeclared() {
    let errors = Column::misdeclared(
        &TextColumn::computed("Status", |u: &User| u.name.clone()).searchable(),
    )
    .into_iter()
    .collect::<Vec<_>>();
    assert!(
        errors
            .iter()
            .any(|error| error.contains("searchable() on computed column")),
        "{errors:?}"
    );
}

#[test]
fn computed_sortable_is_misdeclared() {
    let errors =
        Column::misdeclared(&TextColumn::computed("Status", |u: &User| u.name.clone()).sortable())
            .into_iter()
            .collect::<Vec<_>>();
    assert!(
        errors
            .iter()
            .any(|error| error.contains("sortable() on computed column")),
        "{errors:?}"
    );
}

#[test]
fn computed_columns_declare_no_predicate_chrome_agreement() {
    let col = TextColumn::computed("Status", |u: &User| u.name.clone());
    assert!(!col.is_searchable() && !col.is_sortable());
    assert!(col.search_expr("x").is_none());
    assert!(col.order_by(false).is_none());
}

/// A column's kind picks its default width, `.width(..)`
/// overrides it, and the declaration reaches the renderer as data — the
/// CSS it writes on the `th`/`td`, never a Tailwind class.
#[test]
fn text_column_width_defaults_by_kind() {
    let field = TextColumn::r#for(User::fields().name(), |u| u.name.clone());
    assert_eq!(field.column_width(), ColumnWidth::Wide);

    let computed = TextColumn::computed("Status", |u: &User| u.name.clone());
    assert_eq!(computed.column_width(), ColumnWidth::Narrow);

    let declared = computed.width(ColumnWidth::Percent(30));
    assert_eq!(declared.column_width(), ColumnWidth::Percent(30));

    // A wide column declares nothing at all: it takes the share the
    // declared columns leave.
    assert!(ColumnWidth::Wide.explicit_css().is_none());
    assert!(ColumnWidth::Wide.default_percent().is_none());

    // A kind default is a nominal share of the table, resolved by the
    // renderer; an explicit width is emitted as written.
    assert_eq!(ColumnWidth::Narrow.default_percent(), Some(10));
    assert!(ColumnWidth::Narrow.explicit_css().is_none());
    assert_eq!(
        ColumnWidth::Rem(14).explicit_css().as_deref(),
        Some("width: 14rem")
    );
    assert_eq!(
        ColumnWidth::Percent(30).explicit_css().as_deref(),
        Some("width: 30%")
    );
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

    let column = TextColumn::computed("Owner", |p: &Pet| p.id.to_string())
        .include(Pet::fields().owner())
        .include(Pet::fields().vet())
        .include(Pet::fields().owner());
    assert_eq!(
        column.includes().len(),
        2,
        "two relations, one repeated: the column keeps each once"
    );
}
