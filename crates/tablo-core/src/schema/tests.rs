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
        Section::new("S").schema(Grid::new(2).schema(Group::new().schema((
            Field::text(DummyUser::fields().name()),
            Field::file(Doc::fields().path()),
        )))),
    ));
    let names: Vec<&str> = schema.fields().map(Field::name).collect();
    assert_eq!(names, ["title", "name", "path"]);
    assert!(
        schema.fields().any(|field| field.is_file()),
        "a file field nested in a group is in the list"
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

/// A choice's option check reads `value.trim()`, the spelling the record form's parse stores, and
/// a padded value no option matches is still refused rather than trimmed into one.
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
        schema.checked(&cx, &values).await.is_empty(),
        "the option check reads the trimmed value"
    );
    assert_eq!(
        crate::form::parse_scalar::<String>("name", &values, None),
        Ok("red".to_string()),
        "the stored value is the one the check authorised"
    );

    let mut invalid = HashMap::new();
    invalid.insert("name".to_string(), "  re d ".to_string());
    assert!(
        !schema.checked(&cx, &invalid).await.is_empty(),
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

/// A multiple choice over static options, posting `tags`.
fn tags() -> Schema {
    Schema::new(
        Field::choice_input("tags")
            .options(["rust", "async", "sql"])
            .multiple(),
    )
}

/// A multiple choice posts its key once per checked box, after the hidden blank: the fold keeps
/// each value once, in order, and drops the blank; a choice that posts nothing folds nothing.
#[test]
fn a_multiple_choice_folds_what_it_posted_into_one_list() {
    let lists = HashMap::from([(
        "tags".to_string(),
        ["", "sql", " rust ", "sql"].map(String::from).to_vec(),
    )]);
    let mut values = HashMap::from([("tags".to_string(), "sql".to_string())]);
    tags().fold_choices(&mut values, &lists);
    assert_eq!(
        crate::form::parse_list::<String>("tags", &values),
        Ok(vec!["sql".to_string(), "rust".to_string()])
    );
    let mut untouched = HashMap::new();
    tags().fold_choices(&mut untouched, &HashMap::new());
    assert!(untouched.is_empty());
}

/// Each value a multiple choice holds is checked as one choice's would be, and a value that is
/// no list, or lists more than a choice loads, is refused whole.
#[tokio::test]
async fn a_multiple_choice_checks_each_value_it_holds() {
    let cx = crate::test_support::cx();
    let refused = async |value: String| {
        let values = HashMap::from([("tags".to_string(), value)]);
        tags().checked(&cx, &values).await.contains_key("tags")
    };
    let list = |items: &[&str]| {
        crate::form::encode_list(
            &items
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>(),
        )
    };
    assert!(!refused(list(&["rust", "sql"])).await);
    assert!(!refused(list(&[])).await, "none chosen");
    assert!(
        refused(list(&["rust", "go"])).await,
        "a value that is no option"
    );
    assert!(
        refused(list(&["rust"; MAX_RELATIONSHIP_OPTIONS + 1])).await,
        "more than a choice loads"
    );
    assert!(refused("rust".to_string()).await, "a value that is no list");
}
