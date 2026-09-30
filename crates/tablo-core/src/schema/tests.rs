use std::collections::HashMap;

use super::*;
#[derive(Debug, toasty::Model)]
struct DummyUser {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
    #[unique]
    email: String,
}

/// Building a schema resolves every field once into its field list, nested
/// blocks included, so a question about the form's fields reads one list.
#[test]
fn the_field_list_holds_nested_fields_in_declaration_order() {
    #[derive(Debug, toasty::Model)]
    struct Doc {
        #[key]
        #[auto]
        id: uuid::Uuid,
        path: String,
        title: String,
    }
    let schema = Schema::new((
        Field::text(Doc::fields().title()),
        Section::new("S").schema(Grid::new(2).schema(Repeater::new("R").schema((
            Field::text(DummyUser::fields().name()),
            Field::file(Doc::fields().path()),
        )))),
    ));
    let names: Vec<&str> = schema.fields().map(Field::name).collect();
    assert_eq!(names, ["title", "name", "path"]);
    assert!(
        schema.fields().any(|field| field.is_file()),
        "a file field nested in a repeater is in the list"
    );
}

#[test]
#[should_panic(expected = "duplicate field name")]
fn schema_rejects_duplicate_field_names() {
    let _ = Schema::new((
        Field::text(DummyUser::fields().name()),
        Field::text(DummyUser::fields().name()),
    ));
}

#[test]
fn unknown_keys_flags_undeclared_post_keys() {
    let schema = Schema::new(Field::text(DummyUser::fields().name()));
    let mut values = HashMap::new();
    values.insert("name".to_string(), "Ada".to_string());
    values.insert("role".to_string(), "admin".to_string());
    values.insert("confirm".to_string(), "1".to_string());
    assert_eq!(
        schema.unknown_keys(&values),
        vec!["confirm".to_string(), "role".to_string()]
    );
    values.remove("role");
    values.remove("confirm");
    assert!(schema.unknown_keys(&values).is_empty());
}

/// GH #297: a choice's presence and option checks read `value.trim()`,
/// so the trimmed spelling is the one validation authorises. Normalisation
/// writes exactly that value, and a padded value no option matches is still
/// refused rather than trimmed into one.
#[tokio::test]
async fn a_choice_stores_the_value_its_check_authorised() {
    let cx = topcoat::context::CxTestBuilder::new().build();
    let schema = Schema::new(
        Field::choice(DummyUser::fields().name())
            .options(vec!["red".to_string(), "blue".to_string()]),
    );

    let mut values = HashMap::new();
    values.insert("name".to_string(), "  red ".to_string());
    assert!(
        schema.validate_async(&cx, &values).await.is_empty(),
        "the option check reads the trimmed value"
    );
    schema.normalize_values(&mut values);
    assert_eq!(
        values.get("name").map(String::as_str),
        Some("red"),
        "the stored value is the one the check authorised"
    );

    let mut invalid = HashMap::new();
    invalid.insert("name".to_string(), "  re d ".to_string());
    assert!(
        !schema.validate_async(&cx, &invalid).await.is_empty(),
        "trimming does not turn a non-option into one"
    );
}

/// `extend` carries the same duplicate-name guard `Schema::new` does.
#[test]
#[should_panic(expected = "duplicate field name 'name'")]
fn extend_keeps_the_duplicate_field_guard() {
    let input = || Field::text(DummyUser::fields().name());
    let _ = Schema::empty()
        .extend(Schema::new(input()))
        .extend(Schema::new(input()));
}
