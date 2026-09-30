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

#[test]
fn has_file_upload_detects_nested() {
    #[derive(Debug, toasty::Model)]
    struct Doc {
        #[key]
        #[auto]
        id: uuid::Uuid,
        path: String,
        title: String,
    }
    let plain = Schema::new(TextInput::r#for(DummyUser::fields().name()));
    assert!(!plain.has_file_upload());
    let direct = Schema::new(FileUpload::r#for(Doc::fields().path()));
    assert!(direct.has_file_upload());
    // Nested inside Section/Grid/Repeater counts.
    let nested = Schema::new(Section::new("S").schema(Grid::new(2).schema((
        TextInput::r#for(DummyUser::fields().name()),
        FileUpload::r#for(Doc::fields().path()),
    ))));
    assert!(nested.has_file_upload());
    let in_repeater =
        Schema::new(Repeater::new("R").schema(FileUpload::r#for(Doc::fields().path())));
    assert!(in_repeater.has_file_upload());
}

#[test]
#[should_panic(expected = "duplicate field name")]
fn schema_rejects_duplicate_field_names() {
    let _ = Schema::new((
        TextInput::r#for(DummyUser::fields().name()),
        TextInput::r#for(DummyUser::fields().name()),
    ));
}

#[test]
fn unknown_keys_flags_undeclared_post_keys() {
    let schema = Schema::new(TextInput::r#for(DummyUser::fields().name()));
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

/// GH #297: a `Select`'s presence and option checks read `value.trim()`,
/// so the trimmed spelling is the one validation authorises. Normalisation
/// writes exactly that value, and a padded value no option matches is still
/// refused rather than trimmed into one.
#[tokio::test]
async fn a_select_stores_the_value_its_check_authorised() {
    let cx = topcoat::context::CxTestBuilder::new().build();
    let schema = Schema::new(
        Select::r#for(DummyUser::fields().name())
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

/// GH #191: a derived form is built by appending, so `extend` carries the
/// same duplicate-name guard `Schema::new` does.
#[test]
#[should_panic(expected = "duplicate field name 'name'")]
fn extend_keeps_the_duplicate_field_guard() {
    let input = || TextInput::r#for(DummyUser::fields().name());
    let _ = Schema::empty()
        .extend(Schema::new(input()))
        .extend(Schema::new(input()));
}
