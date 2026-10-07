//! The read-only side of a `Schema`: what a detail page renders.
//!
//! These pin the *shape* — the promise that a view is a reading of a record,
//! not a disabled form — because the showcase's HTTP tests can only check that
//! the page answers and shows a value, and the shape is what a detail page
//! would get wrong quietly (an `<input>` in the markup is invisible to a test
//! that greps for the value).

use std::collections::HashMap;

use tablo::schema::{Field, Grid, Group, Schema, Section, Source};
use toasty::Db;
use topcoat::{
    context::{Cx, CxTestBuilder},
    view::ViewExt,
};

#[derive(Debug, toasty::Model)]
struct Doc {
    #[key]
    #[auto]
    id: uuid::Uuid,
    title: String,
    body: String,
    status: String,
    path: String,
}

async fn cx() -> Cx {
    let db = Db::builder()
        .models(toasty::models!(Doc))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    CxTestBuilder::new().app_context(db).build()
}

fn values() -> HashMap<String, String> {
    HashMap::from([
        ("title".to_string(), "A Title".to_string()),
        ("body".to_string(), "Line one\nLine two".to_string()),
        ("status".to_string(), "published".to_string()),
        ("path".to_string(), "/uploads/cover.jpg".to_string()),
    ])
}

/// The rendered page body, as one string.
async fn render(schema: &Schema, values: &HashMap<String, String>) -> String {
    let cx = cx().await;
    schema
        .render(&cx, Source::view(values))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx)
}

#[tokio::test]
async fn a_view_renders_values_not_controls() {
    let schema = Schema::new((
        Field::text(Doc::fields().title()),
        Field::text(Doc::fields().body()).multiline(4),
        Field::choice(Doc::fields().status())
            .options(vec!["draft".to_string(), "published".to_string()]),
        Field::file(Doc::fields().path()),
    ));
    let html = render(&schema, &values()).await;

    assert!(
        !html.contains("<input") && !html.contains("<select") && !html.contains("<textarea"),
        "a view renders values, never controls: {html}"
    );
    assert!(
        !html.contains("required") && !html.contains("aria-invalid"),
        "a view has nothing to validate: {html}"
    );
    // Every declared field is present, with its label and its value.
    for (label, value) in [
        ("Title", "A Title"),
        ("Body", "Line one\nLine two"),
        ("Status", "published"),
        ("Path", "/uploads/cover.jpg"),
    ] {
        assert!(html.contains(label), "missing label {label} in {html}");
        assert!(html.contains(value), "missing value {value} in {html}");
    }
}

#[tokio::test]
async fn a_choice_renders_its_option_label() {
    // A stored key reads as the label the form offered, so the page shows what
    // the user chose rather than the wire value behind it.
    let schema = Schema::new(
        Field::choice(Doc::fields().status())
            .options(vec![("published".to_string(), "Live".to_string())]),
    );
    let html = render(&schema, &values()).await;
    assert!(
        html.contains("Live"),
        "the option label is what a view shows: {html}"
    );
}

#[tokio::test]
async fn a_choice_without_a_matching_option_shows_the_stored_value() {
    // A value the options do not cover (a stale row, a relationship key) still
    // renders: a detail page shows what is stored, and must not blank a field
    // because it cannot name it.
    let schema =
        Schema::new(Field::choice(Doc::fields().status()).options(vec!["draft".to_string()]));
    let html = render(&schema, &values()).await;
    assert!(
        html.contains("published"),
        "an uncovered value renders as itself: {html}"
    );
}

#[tokio::test]
async fn an_empty_value_renders_as_empty() {
    // The framework stores `""` rather than NULL, so a stored record cannot
    // tell "no value" from an empty one — and the page must not imply it can
    // (no "(none)", no dash, no placeholder text).
    let schema = Schema::new(Field::text(Doc::fields().title()));
    let html = render(
        &schema,
        &HashMap::from([("title".to_string(), String::new())]),
    )
    .await;
    // The value node is the innermost `<div>` of the rendered field: located
    // structurally rather than by its utility classes, so a restyle cannot
    // silently turn the lookup into an empty string.
    let start = html.rfind("<div").expect("the render has the value node");
    let open_end = html[start..].find('>').expect("its tag's end") + start + 1;
    let close = html[open_end..].find("</div>").expect("its closing tag") + open_end;
    assert!(
        html[open_end..close].is_empty(),
        "the value is empty: {html}"
    );
    assert!(html.contains("Title"), "the label still renders: {html}");
    assert!(
        !html.contains("(none)")
            && !html.contains("(missing)")
            && !html.contains("placeholder")
            && !html.contains("—"),
        "an empty value is empty, with no invented marker: {html}"
    );
}

/// A field whose key the view values lack is a declaration bug — neither
/// `view_values` nor the record form supplies it — so the page does not
/// render it as an empty value: a debug build fails its `debug_assert!`, and
/// a release build shows `(missing)`, as a list column shows `(unloaded)` for a
/// relation its query did not load (ADR-0018).
#[tokio::test]
#[cfg_attr(
    debug_assertions,
    should_panic(expected = "view field `title` has no value")
)]
async fn a_field_whose_key_the_values_lack_renders_missing() {
    let schema = Schema::new(Field::text(Doc::fields().title()));
    let html = render(&schema, &HashMap::new()).await;
    assert!(html.contains("(missing)"), "got {html}");
}

#[tokio::test]
async fn layout_blocks_keep_their_structure_around_values() {
    let schema = Schema::new(Section::new("Content").schema((
        Group::new().schema(Grid::new(2).schema((
            Field::text(Doc::fields().title()),
            Field::text(Doc::fields().status()),
        ))),
        Field::text(Doc::fields().body()).multiline(4),
    )));
    let html = render(&schema, &values()).await;
    assert!(html.contains("Content"), "section title survives: {html}");
    // What is view-specific is that the values render inside the structure at all.
    assert!(
        html.contains("Title") && html.contains("published") && html.contains("Line one"),
        "every value renders inside the layout blocks: {html}"
    );
    assert!(
        !html.contains("<input"),
        "structure must not reintroduce a control: {html}"
    );
}
