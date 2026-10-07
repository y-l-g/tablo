use std::collections::HashMap;

use super::{
    super::{
        Field,
        test_support::{DummyUser, cx},
    },
    *,
};
use crate::{
    form::FieldErrors,
    schema::{Schema, Source},
};

#[tokio::test]
async fn text_input_renders_with_label_and_ac_field() {
    let cx = cx();
    let schema = Schema::new(Field::text(DummyUser::fields().name()));
    let html = schema
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("data-slot=\"field\"") && html.contains("data-slot=\"field-label\""),
        "missing field/field-label markup in {html}"
    );
    assert!(
        html.contains("name=\"name\""),
        "missing name attr in {html}"
    );
    assert!(html.contains("<input"), "missing input in {html}");
    assert!(html.contains("<label"), "missing label in {html}");
    assert!(
        html.contains("for=\"name\""),
        "missing for/id linking in {html}"
    );
    assert!(
        !html.contains("role=\"alert\""),
        "a valid field must not render an error slot in {html}"
    );
    assert!(html.contains(">Name"), "missing label in {html}");
}

#[tokio::test]
async fn text_input_error_marks_the_field_invalid() {
    // topcoat#420 pins `aria-invalid` and the error slot.
    let cx = cx();
    let schema = Schema::new(Field::text(DummyUser::fields().name()).required());
    let mut errors = FieldErrors::new();
    errors.add("name", "name is required");
    let html = schema
        .render(&cx, Source::form(&HashMap::new(), &errors))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("data-invalid=\"true\"") && html.contains("ac-field--error"),
        "missing invalid field state in {html}"
    );
    assert!(
        html.contains("aria-invalid=\"true\"") && html.contains("aria-describedby=\"name-error\""),
        "missing aria invalid/described-by in {html}"
    );
    assert!(
        html.contains("id=\"name-error\"") && html.contains("name is required"),
        "missing error slot content in {html}"
    );
}

#[tokio::test]
async fn text_input_required_renders_star_and_email_type() {
    let cx = cx();
    let html_req = Schema::new(Field::text(DummyUser::fields().name()).required())
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html_req.contains(">*</span>"),
        "required should render its asterisk in {html_req}"
    );
    assert!(
        html_req.contains("required"),
        "required attr missing in {html_req}"
    );
    assert!(
        html_req.contains("aria-required"),
        "aria-required missing in {html_req}"
    );
    assert!(
        html_req.contains("for=\"name\"") && html_req.contains("id=\"name\""),
        "for/id linking missing in {html_req}"
    );
    let html_email = Schema::new(Field::text(DummyUser::fields().email()).email())
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html_email.contains("type=\"email\"") && !html_email.contains("r#type"),
        "email should render type=email, not r#type=email, in {html_email}"
    );
    let html_text = Schema::new(Field::text(DummyUser::fields().name()))
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html_text.contains("type=\"text\"") && !html_text.contains("r#type"),
        "plain should render type=text, not r#type=text, in {html_text}"
    );
    assert!(
        !html_req.contains("role=\"alert\""),
        "a valid required field must not render an error slot in {html_req}"
    );
}

#[test]
fn text_input_email_validates() {
    let input: Field = Field::text(DummyUser::fields().email()).email().into();
    assert!(
        input.check("not-an-email").is_some(),
        "email should reject invalid"
    );
    assert!(input.check("a@").is_some(), "email should reject partial");
    assert!(
        input.check("a@b.com").is_none(),
        "email should accept valid"
    );
    assert!(input.check(" a@b.com ").is_none(), "email should trim");
}

/// The email rule is `email_address`.
#[test]
fn text_input_email_edges() {
    let input: Field = Field::text(DummyUser::fields().email()).email().into();
    for ok in [
        "a@b.com",
        "user+tag@sub.example.co",
        "Ada@Example.COM",
        "用户@例え.jp",
        "\"a b\"@example.com",
        "a@[IPv6:::1]",
        "user@my_host.com",
        "a@b.c",
    ] {
        assert!(input.check(ok).is_none(), "{ok} should pass");
    }
    for bad in [
        "a@b".to_string(),
        "a@b..c".to_string(),
        "a b@c.com".to_string(),
        "Ada Lovelace <ada@example.com>".to_string(),
        "not-an-email".to_string(),
        "a@".to_string(),
        "a@b.c.".to_string(),
        ".a@b.com".to_string(),
        "a.@b.com".to_string(),
        "a@@b.com".to_string(),
        "a@-b.com".to_string(),
        "a@b-.com".to_string(),
        "a,b@b.com".to_string(),
        "a(b@b.com".to_string(),
        "a@b!.com".to_string(),
        format!("{}@b.com", "a".repeat(65)),
        format!("a@{}.com", "b".repeat(64)),
        format!(
            "{}@{}.{}.{}",
            "a".repeat(64),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(62)
        ),
    ] {
        assert!(input.check(&bad).is_some(), "{bad} should fail");
    }
}

#[tokio::test]
async fn multiline_renders_a_textarea_with_the_stored_value() {
    // A `<textarea>` takes its value from content, not a `value` attribute.
    let cx = cx();
    let schema = Schema::new(Field::text(DummyUser::fields().name()).multiline(4));
    let mut values = HashMap::new();
    values.insert("name".to_string(), "Line one\nLine two".to_string());
    let html = schema
        .render(&cx, Source::form(&values, &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("<textarea"),
        "a multi-line field must render a textarea control, got {html}"
    );
    assert!(
        !html.contains("<input"),
        "a multi-line field must not render an input, got {html}"
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
        "the field family chrome must match a one-line field, got {html}"
    );
}
