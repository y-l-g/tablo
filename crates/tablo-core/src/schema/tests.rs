use std::collections::HashMap;

use super::*;
use crate::{DeclarationErrorKind, test_support::DummyUser};

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
fn schema_records_duplicate_field_names() {
    let errors = Schema::new((
        Field::text(DummyUser::fields().name()),
        Field::text(DummyUser::fields().name()),
    ))
    .declaration_errors();
    assert_eq!(
        errors,
        [DeclarationErrorKind::DuplicateField {
            name: "name".to_string()
        }]
    );
}

/// Rendering a misdeclared schema fails with its declaration errors rather
/// than controls that lie.
#[tokio::test]
async fn a_misdeclared_schema_fails_to_render() {
    let schema = Schema::new((
        Field::text(DummyUser::fields().name()),
        Field::text(DummyUser::fields().name()),
    ));
    let cx = topcoat::context::CxTestBuilder::new().build();
    let values = HashMap::new();
    let errors = crate::form::FieldErrors::new();
    let Err(error) = schema.render(&cx, Source::form(&values, &errors)).await else {
        panic!("a misdeclared schema must not render");
    };
    assert!(
        error.to_string().contains("two fields are named 'name'"),
        "a duplicate field must name the field, got {error}"
    );
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

/// A choice's presence and option checks read `value.trim()`,
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
fn extend_keeps_the_duplicate_field_check() {
    let input = || Field::text(DummyUser::fields().name());
    let errors = Schema::empty()
        .extend(Schema::new(input()))
        .extend(Schema::new(input()))
        .declaration_errors();
    assert_eq!(
        errors,
        [DeclarationErrorKind::DuplicateField {
            name: "name".to_string()
        }]
    );
}
