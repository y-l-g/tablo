use std::collections::HashMap;

use super::*;
use crate::{
    form::FieldErrors,
    schema::{Group, Schema, Section},
    test_support::{DummyUser, Html as _, cx},
};

#[tokio::test]
async fn schema_composes_multiple_blocks() {
    let cx = cx();
    let schema = Schema::new((
        Section::new("A").schema(Field::text(DummyUser::fields().name())),
        Group::new().schema(Field::text(DummyUser::fields().email())),
    ));
    let html = schema
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .html(&cx)
        .await;
    // The assertions below check structure, not paint: each block
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
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .html(&cx)
        .await;
    assert!(
        html.trim().is_empty(),
        "empty schema should render nothing, got {html}"
    );
}

/// A field inside nested blocks keeps its slot when the blocks join a
/// schema that already holds fields: every field renders under its own name.
#[tokio::test]
async fn nested_blocks_keep_their_field_slots() {
    let cx = cx();
    let schema = Schema::new((
        Field::text(DummyUser::fields().name()),
        Section::new("S").schema(Group::new().schema(Field::text(DummyUser::fields().email()))),
    ));
    assert_eq!(
        schema.fields().map(Field::name).collect::<Vec<_>>(),
        ["name", "email"],
        "the field list holds every field once, in declaration order"
    );
    let values = HashMap::from([
        ("name".to_string(), "Ada".to_string()),
        ("email".to_string(), "ada@example.com".to_string()),
    ]);
    let html = schema
        .render(&cx, Source::form(&values, &FieldErrors::new()))
        .await
        .html(&cx)
        .await;
    assert!(html.contains("value=\"Ada\""), "{html}");
    assert!(html.contains("value=\"ada@example.com\""), "{html}");
}
