use std::collections::HashMap;

use super::{
    super::{
        Field,
        test_support::{DummyUser, NullableRef, cx},
    },
    *,
};
use crate::{
    form::FieldErrors,
    schema::{Schema, Source},
};

/// The messages `errors` carries, in the order the rules reported them.
fn messages(errors: &[crate::form::FieldError]) -> Vec<&str> {
    errors.iter().map(|error| error.message.as_str()).collect()
}

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

/// A typed field shows its stored value on a detail page.
#[tokio::test]
async fn a_typed_field_renders_its_stored_value_read_only() {
    const ID: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";
    let cx = cx();
    let schema = Schema::new(Field::text(DummyUser::fields().id()));
    let mut values = HashMap::new();
    values.insert("id".to_string(), ID.to_string());
    let html = schema
        .render(&cx, Source::view(&values))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains(ID),
        "a detail page must show a typed field's stored value, got {html}"
    );
    assert!(
        !html.contains("<input"),
        "and must render no control, got {html}"
    );
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

#[test]
fn text_input_required_validates_empty() {
    let input = Field::text(DummyUser::fields().name()).required();
    assert!(
        !input.validate("").is_empty(),
        "required should reject empty"
    );
    assert!(
        input.validate("hello").is_empty(),
        "required should accept non-empty"
    );
    assert!(
        !input.validate("   ").is_empty(),
        "required should reject whitespace"
    );
    assert!(
        !Field::text(DummyUser::fields().name())
            .validate("")
            .is_empty(),
        "non-nullable columns default to required"
    );
    assert!(
        Field::text(DummyUser::fields().name())
            .optional()
            .validate("")
            .is_empty(),
        "optional should accept empty"
    );
}

#[test]
fn required_default_follows_lens_nullability() {
    #[derive(Debug, toasty::Model)]
    struct NullableDoc {
        #[key]
        #[auto]
        id: uuid::Uuid,
        nick: Option<String>,
    }
    assert!(
        Field::choice(NullableDoc::fields().nick())
            .validate("")
            .is_empty(),
        "nullable columns default to optional"
    );
    assert!(
        !Field::text(DummyUser::fields().name())
            .validate("")
            .is_empty(),
        "String columns are non-nullable, empty must fail inline"
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
    let input = Field::text(DummyUser::fields().email()).required().email();
    assert!(
        !input.validate("not-an-email").is_empty(),
        "email should reject invalid"
    );
    assert!(
        !input.validate("a@").is_empty(),
        "email should reject partial"
    );
    assert!(
        input.validate("a@b.com").is_empty(),
        "email should accept valid"
    );
    // `DummyUser.email` is unique, so `.optional` cannot lift presence there.
    assert!(
        Field::choice(NullableRef::fields().parent_id())
            .optional()
            .validate("")
            .is_empty(),
        "an optional, non-unique field must still accept empty"
    );
    assert!(
        Field::text(DummyUser::fields().email())
            .email()
            .validate(" a@b.com ")
            .is_empty(),
        "email should trim"
    );
}

/// The unique marker is presence, so `.optional()` cannot lift it
/// — in the builder or from the lens.
#[test]
fn unique_implies_required_in_either_declaration_order() {
    let mut declarations = vec![
        Field::text(DummyUser::fields().email()).optional().unique(),
        Field::text(DummyUser::fields().email()).unique().optional(),
    ];
    declarations.push(Field::text(DummyUser::fields().email()).optional());

    for (nth, input) in declarations.iter().enumerate() {
        assert!(
            input.is_unique() && input.is_required(),
            "declaration {nth} must be unique and required"
        );
        assert_eq!(
            messages(&input.validate("")),
            ["Email is required"],
            "declaration {nth}: an empty unique field is required, not absent"
        );
        assert_eq!(
            messages(&input.validate("   ")),
            ["Email is required"],
            "declaration {nth}: whitespace-only counts as empty, as everywhere else"
        );
        assert!(
            input.validate("a@b.com").is_empty(),
            "declaration {nth}: a present value still validates normally"
        );
    }
}

/// A unique field renders the required marker.
#[tokio::test]
async fn unique_field_renders_the_required_marker() {
    let cx = cx();
    let html = Schema::new(Field::text(DummyUser::fields().email()).unique().optional())
        .render(&cx, Source::form(&HashMap::new(), &FieldErrors::new()))
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    assert!(
        html.contains("required") && html.contains("aria-required"),
        "a unique field is required in the markup too"
    );
    assert!(
        html.contains(">*</span>"),
        "the required asterisk must render"
    );
}

/// The email rule is `email_address`.
#[test]
fn text_input_email_edges() {
    let input = Field::text(DummyUser::fields().email()).email();
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
        assert!(input.validate(ok).is_empty(), "{ok} should pass");
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
        assert!(!input.validate(&bad).is_empty(), "{bad} should fail");
    }
}

/// An empty submit is the presence rule's business: the email rule skips
/// it, and presence reports first.
#[test]
fn email_rule_leaves_an_empty_value_to_presence() {
    let input = Field::text(DummyUser::fields().email()).required().email();
    let empty = input.validate("");
    assert!(
        messages(&empty).len() == 1 && messages(&empty)[0].contains("Email"),
        "an empty value is presence's business, got {empty:?}"
    );
    let blank = input.validate("   ");
    assert!(
        messages(&blank).len() == 1 && messages(&blank)[0].contains("Email"),
        "whitespace-only counts as empty, got {blank:?}"
    );
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
