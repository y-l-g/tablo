use std::collections::HashMap;

use super::{
    super::test_support::{DummyUser, cx},
    *,
};
use crate::schema::Schema;

#[tokio::test]
async fn textarea_renders_a_multiline_control_with_the_stored_value() {
    // GH #184: prose columns get a `<textarea>`, not a one-line input. The
    // value is the control's child — a textarea has no `value` attribute.
    let cx = cx();
    let schema = Schema::new(Textarea::r#for(DummyUser::fields().name()).rows(4));
    let mut values = HashMap::new();
    values.insert("name".to_string(), "Line one\nLine two".to_string());
    let html = schema
        .render_with(&cx, &values, &HashMap::new())
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("<textarea"),
        "Textarea must render a textarea control, got {html}"
    );
    assert!(
        !html.contains("<input"),
        "a Textarea must not render an input, got {html}"
    );
    assert!(
        html.contains("Line one") && html.contains("Line two"),
        "the stored value must be the control's content, got {html}"
    );
    assert!(
        html.contains("rows=\"4\""),
        "declared rows must reach the control, got {html}"
    );
    assert!(
        html.contains(">Name"),
        "label must still derive from the lens, got {html}"
    );
    assert!(
        html.contains("data-slot=\"field\""),
        "the field family chrome must match TextInput, got {html}"
    );
}

#[tokio::test]
async fn textarea_shares_the_required_contract_with_text_input() {
    // The two text fields differ in control only: presence validation and
    // the required marker follow the same lens-derived default.
    let schema = Schema::new(Textarea::r#for(DummyUser::fields().name()));
    let errors = schema.validate(&HashMap::new());
    assert_eq!(
        errors.get("name"),
        Some(&vec!["Name is required".to_string()]),
        "a non-nullable String column is required by default, as TextInput"
    );

    let optional = Schema::new(Textarea::r#for(DummyUser::fields().name()).optional());
    assert!(
        optional.validate(&HashMap::new()).is_empty(),
        "optional() must opt out of the required default"
    );

    // An omitted key is validated as empty, matching TextInput.
    let whitespace = schema.validate(&HashMap::from([("name".to_string(), "   ".to_string())]));
    assert_eq!(
        whitespace.get("name"),
        Some(&vec!["Name is required".to_string()]),
        "whitespace-only counts as empty, matching the trim convention"
    );
}
