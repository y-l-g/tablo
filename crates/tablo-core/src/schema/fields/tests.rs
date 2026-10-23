use jiff::tz::TimeZone;

use super::{test_support::DummyUser, *};
use crate::{
    DeclarationErrorKind,
    schema::Schema,
    test_support::cx,
    values::{FormValue, FormValues},
};

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

/// A toggle turned into a choice posts the choice's options, not a checkbox's `true` or `false`.
#[test]
fn a_toggle_turned_into_a_choice_takes_its_options() {
    let field: Field = Field::toggle_input("plan")
        .choice()
        .options(["monthly", "yearly"])
        .into();
    assert!(!field.refuses(&FormValue::from("yearly")));
    assert!(field.refuses(&FormValue::from("true")));
}

/// The email rule refuses a non-empty value that is not an address, and leaves presence to the
/// record form.
#[test]
fn the_email_rule_checks_only_a_submitted_value() {
    let field: Field = Field::text(DummyUser::fields().email()).email().into();
    assert_eq!(
        field.check(&FormValue::from(""), &TimeZone::UTC),
        None,
        "an empty value is the record form's"
    );
    assert_eq!(
        field.check(&FormValue::from("ada@example.com"), &TimeZone::UTC),
        None
    );
    assert_eq!(
        field
            .check(&FormValue::from("nope"), &TimeZone::UTC)
            .map(|error| error.message("Email").english()),
        Some("Email must be a valid email".to_string())
    );
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
    let posted = [
        ("reason".to_string(), "other".to_string()),
        ("note".to_string(), "hello".to_string()),
    ];
    let input = schema.read_input(&cx(), &posted, &[]).expect("reads");
    assert_eq!(input.text("reason"), Some("spam"));
    assert_eq!(input.text("note"), Some("hello"));
    assert_eq!(schema.defaults().len(), 2);
}

/// A default the control never posts, one value on a multiple choice and a list elsewhere
/// included, and a row control a repeater cannot disable or default, refuse the declaration.
#[test]
fn a_default_the_control_refuses_and_a_modified_row_control_refuse_the_schema() {
    use crate::DeclarationErrorKind;

    let errors = Schema::new((
        Field::text_input::<i64>("count").default("ten"),
        Field::text_input::<i64>("fine").default(3_i64),
        Field::choice_input("plan").options(["free"]).default("pro"),
        Field::choice_input("tags")
            .options(["free"])
            .multiple()
            .default("free"),
        Field::choice_input("chosen")
            .options(["free"])
            .multiple()
            .default(["free"]),
        Field::choice_input("unoffered")
            .options(["free"])
            .multiple()
            .default(["free", "pro"]),
        Field::text_input::<String>("listed").default(["a"]),
    ))
    .declaration_errors();
    let refused: Vec<_> = errors
        .iter()
        .filter_map(|error| match error {
            DeclarationErrorKind::UnpostedDefault { field, .. } => Some(field.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        refused,
        ["count", "plan", "tags", "unoffered", "listed"],
        "{errors:?}"
    );

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

/// A row's password would render empty on every edit and overwrite the stored secret on save.
#[test]
fn a_repeater_refuses_a_password_in_its_row() {
    fn credential() -> Schema {
        Schema::new((
            Field::text_input::<String>("host"),
            Field::text_input::<String>("token").password(),
        ))
    }
    let repeater = Field::bound(
        Field::named("credentials".to_string()),
        ControlKind::Repeater(super::super::repeater::RepeaterControl::of(credential)),
    );
    let errors = Schema::new(repeater).declaration_errors();
    assert!(
        errors.iter().any(|error| matches!(
            error,
            crate::DeclarationErrorKind::RepeaterPassword { field, leaf }
                if field == "credentials" && leaf == "token"
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
    let posted = [
        ("plan".to_string(), "pro".to_string()),
        ("coupon".to_string(), "SAVE".to_string()),
        ("referral".to_string(), "eve".to_string()),
    ];
    let input = schema.read_input(&cx(), &posted, &[]).expect("reads");
    assert_eq!(input.text("plan"), Some("free"));
    assert_eq!(input.text("coupon"), Some("SAVE"));
    assert_eq!(input.get("referral"), None, "hidden by the default");
}

/// A row's field compares with a field of the same row, under that row's keys, and the mount
/// reports a row's comparison under the repeater's key.
#[tokio::test]
async fn a_repeater_row_compares_its_own_fields() {
    fn leg() -> Schema {
        let low = Field::text_input::<i64>("low");
        let high = Field::text_input::<i64>("high").gt(&low);
        let ghost = Field::text_input::<i64>("ghost");
        Schema::new((low, high, Field::text_input::<i64>("far").lt(&ghost)))
    }
    let schema = Schema::new(Field::bound(
        Field::named("legs".to_string()),
        ControlKind::Repeater(super::super::repeater::RepeaterControl::of(leg)),
    ));
    assert_eq!(
        schema.declaration_errors(),
        [DeclarationErrorKind::FieldRule {
            field: "legs.far".to_string(),
            fault: crate::FieldRuleFault::UnplacedComparedField("ghost".to_string()),
        }]
    );
    let row = |low: &str, high: &str| FormValues::from_iter([("low", low), ("high", high)]);
    let values =
        FormValues::from_iter([("legs", FormValue::Rows(vec![row("1", "2"), row("3", "3")]))]);
    let errors = schema.checked(&cx(), &values).await;
    let keys: Vec<&str> = errors.iter().map(|error| error.key.as_str()).collect();
    assert_eq!(keys, ["legs.1.high"]);
}
