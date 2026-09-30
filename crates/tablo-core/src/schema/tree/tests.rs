use std::collections::HashMap;

use super::*;
use crate::{
    schema::{Group, Schema, Section, TextInput},
    test_support::cx,
};

#[derive(Debug, toasty::Model)]
struct DummyUser {
    #[key]
    #[auto]
    id: uuid::Uuid,
    name: String,
    #[unique]
    email: String,
}

/// GH #297: a variant group the submission's discriminant does not name is
/// the one `variant.js` hides, so its fields cannot fail the submit. The
/// named variant's fields still validate, and a submission that names no
/// variant hides nothing — the value codec's payload fallback may still
/// read any group, so every group stays checked.
#[test]
fn a_hidden_variant_group_is_not_validated() {
    let schema = Schema::new((
        // The discriminant carrier: the marker's owner names it, exactly as
        // a derived enum's variant `Select` does.
        TextInput::r#for(DummyUser::fields().name()).label("Kind"),
        Group::new()
            .variant("name", "1")
            .schema(TextInput::r#for(DummyUser::fields().email()).email()),
        Group::new()
            .variant("name", "2")
            .schema(TextInput::typed::<DummyUser, uuid::Uuid>(
                DummyUser::fields().id(),
            )),
    ));
    let mut values = HashMap::new();
    values.insert("name".to_string(), "2".to_string());
    values.insert("email".to_string(), "not-an-email".to_string());
    values.insert(
        "id".to_string(),
        "0f8fad5b-d9cb-469f-a165-70867728950e".to_string(),
    );

    let errors = schema.validate(&values);
    assert!(
        !errors.contains_key("email"),
        "a hidden variant's field must not block the submit, got {errors:?}"
    );

    // The named variant's own fields validate as usual.
    let mut named = values.clone();
    named.insert("name".to_string(), "1".to_string());
    let errors = schema.validate(&named);
    assert!(
        errors.contains_key("email"),
        "the named variant's field must still validate, got {errors:?}"
    );

    // No variant named: the group set is not narrowed, so the invalid
    // value the payload fallback could read is refused.
    let mut unnamed = values.clone();
    unnamed.insert("name".to_string(), String::new());
    let errors = schema.validate(&unnamed);
    assert!(
        errors.contains_key("email"),
        "an unnamed submission validates every variant's fields, got {errors:?}"
    );
}

#[tokio::test]
async fn text_input_composes_in_tuple() {
    let cx = cx();
    let schema = Schema::new((
        TextInput::r#for(DummyUser::fields().name()),
        TextInput::r#for(DummyUser::fields().email()),
    ));
    let html = schema
        .render(&cx)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.matches("data-slot=\"field\"").count() >= 2,
        "expected 2 fields (data-slot=field) in {html}"
    );
    assert_eq!(
        html.matches("role=\"alert\"").count(),
        0,
        "valid fields render no error slot in {html}"
    );
}

#[tokio::test]
async fn schema_composes_multiple_blocks() {
    let cx = cx();
    let schema = Schema::new((
        Section::new("A").schema(TextInput::r#for(DummyUser::fields().name())),
        Group::new().schema(TextInput::r#for(DummyUser::fields().email())),
    ));
    let html = schema
        .render(&cx)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    // GH #216: the assertions below check structure, not paint: each block
    // renders its own child, exactly once, and the section's title still
    // frames its field.
    assert!(html.contains("A"), "missing section title in {html}");
    assert_eq!(
        html.matches("data-slot=\"field\"").count(),
        2,
        "each block must render its own field, got {html}"
    );
    assert!(
        html.contains("name=\"name\"") && html.contains("name=\"email\""),
        "both block children must render, got {html}"
    );
    assert!(
        html.find("A").expect("the section title") < html.find("name=\"name\"").expect("its field"),
        "the section must frame the field it holds, got {html}"
    );
}

#[tokio::test]
async fn empty_schema_renders_empty() {
    let cx = cx();
    let schema = Schema::empty();
    let html = schema
        .render(&cx)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.trim().is_empty(),
        "empty schema should render nothing, got {html}"
    );
}
