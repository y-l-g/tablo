use super::{test_support::DummyUser, *};

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
