use super::*;
use crate::{DeclarationErrorKind, test_support::DummyUser};

/// Building a schema resolves every field once into its field list, nested
/// blocks included, so a question about the form's fields reads one list.
#[test]
fn the_field_list_holds_nested_fields_in_declaration_order() {
    #[derive(Debug, toasty::Model, crate::ModelColumns)]
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
    let values = FormValues::new();
    let errors = crate::form::FieldErrors::new();
    let Err(error) = schema.render(&cx, Source::form(&values, &errors)).await else {
        panic!("a misdeclared schema must not render");
    };
    assert!(
        error.to_string().contains("two fields are named 'name'"),
        "a duplicate field must name the field, got {error}"
    );
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

    let values = FormValues::from_iter([("name", "  red ")]);
    assert!(
        schema.checked(&cx, &values).await.is_empty(),
        "the option check reads the trimmed value"
    );
    assert_eq!(
        crate::form::parse_scalar::<String>("name", &values, None),
        Ok("red".to_string()),
        "the stored value is the one the check authorised"
    );

    let invalid = FormValues::from_iter([("name", "  re d ")]);
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
pub(super) fn tags() -> Schema {
    Schema::new(
        Field::choice_input("tags")
            .options(["rust", "async", "sql"])
            .multiple(),
    )
}

/// Each value a multiple choice holds is checked as one choice's would be, and a list choosing
/// more than a choice loads is refused whole.
#[tokio::test]
async fn a_multiple_choice_checks_each_value_it_holds() {
    let cx = crate::test_support::cx();
    let refused = async |items: Vec<String>| {
        let values = FormValues::from_iter([("tags", items)]);
        tags().checked(&cx, &values).await.contains_key("tags")
    };
    let list = |items: &[&str]| items.iter().map(|item| item.to_string()).collect();
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
}

/// A multiple choice caps the choices a submission adds, not the ones the record holds already:
/// a record holding more of them than a choice loads still saves.
#[tokio::test]
async fn a_multiple_choice_caps_only_the_choices_it_adds() {
    let cx = crate::test_support::cx();
    let held: Vec<String> = (0..=MAX_RELATIONSHIP_OPTIONS)
        .map(|key| format!("rust{key}"))
        .collect();
    let refused = async |items: Vec<String>| {
        let values = FormValues::from_iter([("tags", items)]);
        let stored = FormValues::from_iter([("tags", held.clone())]);
        let mut errors = crate::FieldErrors::new();
        tags()
            .check_controls(&cx, &values, &stored, &mut errors)
            .await;
        errors.contains_key("tags")
    };
    assert!(
        !refused(held.clone()).await,
        "a record keeping the choices it holds saves"
    );
    let mut added = held.clone();
    added.push("sql".to_string());
    assert!(
        !refused(added).await,
        "one choice beside them is not past it"
    );
    let mut many = held.clone();
    many.extend((0..=MAX_RELATIONSHIP_OPTIONS).map(|key| format!("sql{key}")));
    assert!(
        refused(many).await,
        "more choices than a choice loads is refused"
    );
}

/// An error renders under its field's own key; a row key or a near-miss renders nowhere.
#[test]
fn an_error_no_field_renders_is_unplaced() {
    let schema = Schema::new(Field::text(DummyUser::fields().name()));
    let unplaced = |key: &str| {
        let mut errors = crate::FieldErrors::new();
        errors.add(key, "refused");
        schema.unplaced_error(&errors).map(str::to_string)
    };
    assert_eq!(unplaced("name"), None);
    assert_eq!(
        unplaced("name.0.label").as_deref(),
        Some("name.0.label"),
        "a text field posts no rows"
    );
    assert_eq!(unplaced("names").as_deref(), Some("names"));
}

/// A multiple choice's list default checks its boxes on the create form.
#[tokio::test]
async fn a_multiple_choices_list_default_checks_its_boxes() {
    use crate::test_support::Html;

    let schema = Schema::new(
        Field::choice_input("tags")
            .options(["rust", "async", "sql"])
            .multiple()
            .default(["rust", "sql"]),
    );
    let cx = crate::test_support::cx();
    let (values, errors) = (schema.defaults(), crate::form::FieldErrors::new());
    let html = schema
        .render(&cx, Source::form(&values, &errors))
        .await
        .html(&cx)
        .await;
    let checked: Vec<&str> = ["rust", "async", "sql"]
        .into_iter()
        .filter(|tag| {
            html.split("<input").any(|input| {
                input.contains(&format!("value=\"{tag}\"")) && input.contains("checked=\"\"")
            })
        })
        .collect();
    assert_eq!(checked, ["rust", "sql"], "{html}");
}
