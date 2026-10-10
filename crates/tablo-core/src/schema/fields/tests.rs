use std::collections::HashMap;

use super::{test_support::DummyUser, *};
use crate::schema::Schema;

/// A control renders optional until a panel stamps its record form's presence on it.
#[test]
fn a_control_renders_optional_until_its_record_form_requires_it() {
    assert!(!Field::text(DummyUser::fields().name()).is_required());
    assert!(!Field::choice(DummyUser::fields().name()).is_required());
    assert!(!Field::file(DummyUser::fields().name()).is_required());
    assert!(
        Field::text(DummyUser::fields().name())
            .required()
            .is_required()
    );
}

#[derive(Debug, toasty::Model)]
struct Coded {
    #[key]
    #[auto]
    id: uuid::Uuid,
    #[unique]
    code: i64,
    #[unique]
    nickname: Option<String>,
}

/// A unique index marks the field unique by default, whatever the scalar
/// type, so a typed column is probed like a `String` one.
#[test]
fn a_typed_unique_column_defaults_to_unique() {
    assert!(Field::text(Coded::fields().code()).is_unique());
    assert!(Field::text(Coded::fields().nickname()).is_unique());
    assert!(!Field::text(DummyUser::fields().name()).is_unique());
}

/// A nullable column stores NULL for an empty submission, which a unique index admits many times;
/// a non-nullable one stores one value for all of them.
#[test]
fn a_field_reads_its_columns_nullability() {
    assert!(Field::text(Coded::fields().nickname()).is_nullable());
    assert!(!Field::text(Coded::fields().code()).is_nullable());
}

/// The email rule refuses a non-empty value that is not an address, and leaves presence to the
/// record form.
#[test]
fn the_email_rule_checks_only_a_submitted_value() {
    let field: Field = Field::text(DummyUser::fields().email()).email().into();
    assert_eq!(field.check(""), None, "an empty value is the record form's");
    assert_eq!(field.check("ada@example.com"), None);
    assert_eq!(
        field.check("nope").map(|error| error.message("Email")),
        Some("Email must be a valid email".to_string())
    );
}

/// Each control renders `disabled` on the form its modifier names, and every part that posts is
/// disabled with it.
#[tokio::test]
async fn every_control_renders_disabled_where_its_modifier_says() {
    use crate::{form::FieldErrors, schema::Source, test_support::Html as _};

    async fn html(schema: Schema, editing: bool) -> String {
        let cx = test_support::cx();
        let values = HashMap::from([("name".to_string(), "/uploads/a.png".to_string())]);
        schema
            .render(
                &cx,
                Source::form(&values, &FieldErrors::new()).editing(editing),
            )
            .await
            .html(&cx)
            .await
    }
    let disabled_count = |html: &str| html.matches("disabled=\"\"").count();

    for (schema, posting) in [
        (
            Schema::new(Field::text(DummyUser::fields().name()).disabled()),
            1,
        ),
        (
            Schema::new(
                Field::text(DummyUser::fields().name())
                    .multiline(3)
                    .disabled(),
            ),
            1,
        ),
        (
            Schema::new(
                Field::choice(DummyUser::fields().name())
                    .options(["a"])
                    .disabled(),
            ),
            1,
        ),
        (
            Schema::new(
                Field::choice(DummyUser::fields().name())
                    .options(["a", "b"])
                    .multiple()
                    .disabled(),
            ),
            1,
        ),
        // The input, without the clear box.
        (
            Schema::new(Field::file(DummyUser::fields().name()).disabled()),
            1,
        ),
        // The hidden `false` and the box.
        (Schema::new(Field::toggle_input("name").disabled()), 2),
    ] {
        let html = html(schema, false).await;
        assert_eq!(disabled_count(&html), posting, "{html}");
    }

    let on_edit = || Schema::new(Field::text(DummyUser::fields().name()).disabled_on_edit());
    assert_eq!(disabled_count(&html(on_edit(), false).await), 0);
    assert_eq!(disabled_count(&html(on_edit(), true).await), 1);
}

/// Help renders under every control, which names it in `aria-describedby` beside its error.
#[tokio::test]
async fn help_text_describes_its_control() {
    use crate::{form::FieldErrors, schema::Source, test_support::Html as _};

    let cx = test_support::cx();
    let values = HashMap::new();
    let mut errors = FieldErrors::new();
    errors.add("name", "Name is wrong");
    for schema in [
        Schema::new(Field::text(DummyUser::fields().name()).help("Your full name")),
        Schema::new(
            Field::choice(DummyUser::fields().name())
                .options(["a"])
                .multiple()
                .help("Your full name"),
        ),
    ] {
        let html = schema
            .render(&cx, Source::form(&values, &errors))
            .await
            .html(&cx)
            .await;
        assert!(
            html.contains("id=\"name-description\"") && html.contains("Your full name"),
            "{html}"
        );
        assert!(
            html.contains("aria-describedby=\"name-description name-error\""),
            "{html}"
        );
    }
}

/// An action's input drops what a disabled control posts and takes its default instead.
#[test]
fn a_read_input_takes_a_disabled_fields_default() {
    let schema = Schema::new((
        Field::text_input::<String>("reason")
            .default("spam")
            .disabled(),
        Field::text_input::<String>("note").default("none"),
    ));
    let posted = HashMap::from([
        ("reason".to_string(), "other".to_string()),
        ("note".to_string(), "hello".to_string()),
    ]);
    let input = schema
        .read_input(&posted, &HashMap::new(), &[])
        .expect("reads");
    assert_eq!(input["reason"], "spam");
    assert_eq!(input["note"], "hello");
    assert_eq!(schema.defaults().len(), 2);
}

/// A default the control never posts, and a row control a repeater cannot disable or default,
/// refuse the declaration.
#[test]
fn a_default_the_control_refuses_and_a_modified_row_control_refuse_the_schema() {
    use crate::DeclarationErrorKind;

    let errors = Schema::new((
        Field::text_input::<i64>("count").default("ten"),
        Field::text_input::<i64>("fine").default(3_i64),
        Field::choice_input("plan").options(["free"]).default("pro"),
    ))
    .declaration_errors();
    let refused: Vec<_> = errors
        .iter()
        .filter_map(|error| match error {
            DeclarationErrorKind::UnpostedDefault { field, .. } => Some(field.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(refused, ["count", "plan"], "{errors:?}");

    fn locked() -> Schema {
        Schema::new(Field::text_input::<String>("label").disabled())
    }
    let repeater = Field::bound(
        Field::named("links".to_string()),
        ControlKind::Repeater(super::super::repeater::RepeaterControl::of(locked)),
    );
    let errors = Schema::new(repeater).declaration_errors();
    assert!(
        errors.iter().any(|error| matches!(
            error,
            DeclarationErrorKind::RepeaterLeafModifier { field, leaf }
                if field == "links" && leaf == "label"
        )),
        "{errors:?}"
    );
}

/// A disabled watched field is read as its default, not as what a submission posts for it.
#[test]
fn a_condition_reads_a_disabled_fields_default() {
    let plan = Field::choice_input("plan")
        .options(["free", "pro"])
        .default("free")
        .disabled();
    let coupon = Field::text_input::<String>("coupon").visible_when(&plan, ["free"]);
    let referral = Field::text_input::<String>("referral").visible_when(&plan, ["pro"]);
    let schema = Schema::new((plan, coupon, referral));
    let posted = HashMap::from([
        ("plan".to_string(), "pro".to_string()),
        ("coupon".to_string(), "SAVE".to_string()),
        ("referral".to_string(), "eve".to_string()),
    ]);
    let input = schema
        .read_input(&posted, &HashMap::new(), &[])
        .expect("reads");
    assert_eq!(input.get("plan").map(String::as_str), Some("free"));
    assert_eq!(input.get("coupon").map(String::as_str), Some("SAVE"));
    assert_eq!(input.get("referral"), None, "hidden by the default");
}
