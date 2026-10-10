use std::collections::HashMap;

use super::super::{
    Field,
    test_support::{DummyUser, cx},
};
use crate::{
    form::FieldErrors,
    schema::{Schema, Source},
    test_support::Html as _,
};

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
        .html(&cx)
        .await;
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

/// A refused form re-renders its submitted values, except a password's.
#[tokio::test]
async fn password_renders_masked_and_never_echoes_its_value() {
    let cx = cx();
    let schema = Schema::new(Field::text_input::<String>("secret").password());
    let mut values = HashMap::new();
    values.insert("secret".to_string(), "hunter22".to_string());
    let html = schema
        .render(&cx, Source::form(&values, &FieldErrors::new()))
        .await
        .html(&cx)
        .await;
    assert!(html.contains("type=\"password\""), "{html}");
    assert!(!html.contains("hunter22"), "{html}");
}

/// Each typed control renders the input its browser edits it with, and a stored value in the
/// spelling that input accepts.
#[tokio::test]
async fn a_typed_field_renders_its_input_type_and_value() {
    async fn input(schema: Schema, stored: &str) -> String {
        let cx = cx();
        let values = HashMap::from([("value".to_string(), stored.to_string())]);
        let html = schema
            .render(&cx, Source::form(&values, &FieldErrors::new()))
            .await
            .html(&cx)
            .await;
        let start = html.find("<input").expect("an input renders");
        html[start..start + html[start..].find('>').unwrap()].to_string()
    }

    let html = input(Schema::new(Field::text_input::<i64>("value")), "42").await;
    assert!(
        html.contains("type=\"number\"") && !html.contains("step="),
        "{html}"
    );
    let html = input(
        Schema::new(Field::text_input::<Option<f64>>("value")),
        "1.5",
    )
    .await;
    assert!(
        html.contains("type=\"number\"") && html.contains("step=\"any\""),
        "{html}"
    );
    let html = input(
        Schema::new(Field::text_input::<f64>("value").password()),
        "1",
    )
    .await;
    assert!(
        !html.contains("step="),
        "a password input takes no step: {html}"
    );

    for (schema, input_type, stored, value) in [
        (
            Schema::new(Field::text_input::<jiff::civil::Date>("value")),
            "date",
            "2024-01-15",
            "2024-01-15",
        ),
        (
            Schema::new(Field::text_input::<jiff::civil::Time>("value")),
            "time",
            "09:30:15",
            "09:30",
        ),
        (
            Schema::new(Field::text_input::<Option<jiff::civil::DateTime>>("value")),
            "datetime-local",
            "2024-01-15T09:30:00",
            "2024-01-15T09:30",
        ),
        (
            Schema::new(Field::text_input::<jiff::Timestamp>("value")),
            "datetime-local",
            "2024-01-15T09:30:00Z",
            "2024-01-15T09:30",
        ),
    ] {
        let html = input(schema, stored).await;
        assert!(html.contains(&format!("type=\"{input_type}\"")), "{html}");
        assert!(html.contains(&format!("value=\"{value}\"")), "{html}");
    }
}
