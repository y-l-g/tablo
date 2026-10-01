use super::{
    test_support::{DummyUser, NullableRef},
    *,
};

/// Every control defaults `required` from the lens's nullability:
/// non-nullable columns default required, `.optional()` opts out, and
/// `.required()` forces it back.
#[test]
fn every_control_defaults_required_from_nullability() {
    macro_rules! check {
        ($field:expr) => {{
            let field = $field;
            assert!(
                field
                    .validate("")
                    .iter()
                    .any(|e| e.message.contains("is required")),
                "a non-nullable {:?} refuses an empty submit",
                *field
            );
            let field = field.optional();
            assert!(field.validate("").is_empty(), "{:?} opted out", *field);
            let field = field.required();
            assert!(
                field
                    .validate("")
                    .iter()
                    .any(|e| e.message.contains("is required")),
                "{:?} opted back in",
                *field
            );
        }};
    }
    check!(Field::text(DummyUser::fields().name()));
    check!(Field::choice(DummyUser::fields().name()));
    check!(Field::file(DummyUser::fields().name()));

    // A nullable lens defaults optional, whatever the control.
    let choice = Field::choice(NullableRef::fields().parent_id());
    assert!(
        choice.validate("").is_empty(),
        "nullable FK choice defaults optional, got {:?}",
        choice.validate("")
    );
    let text = Field::text(NullableRef::fields().parent_id());
    assert!(
        text.validate("").is_empty(),
        "an `Option` column is optional"
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

/// Uniqueness implies presence only where an empty submit stores a value: a
/// nullable column stores NULL, which a unique index admits many times.
#[test]
fn a_nullable_unique_column_may_be_left_empty() {
    let nullable = Field::text(Coded::fields().nickname());
    assert!(
        nullable.validate("").is_empty(),
        "an `Option` column stays optional: {:?}",
        nullable.validate("")
    );
    assert!(!nullable.is_required(), "and renders no required marker");

    let non_nullable = Field::text(DummyUser::fields().email()).optional();
    assert!(
        non_nullable
            .validate("")
            .iter()
            .any(|e| e.message.contains("is required")),
        "a non-nullable unique column stays required even when optional"
    );
}
