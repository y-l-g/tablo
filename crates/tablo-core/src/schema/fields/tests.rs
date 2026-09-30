use super::{
    test_support::{DummyUser, NullableRef},
    *,
};

/// Every control defaults `required` from the lens's nullability:
/// non-nullable columns default required, `.optional()` opts out, and
/// `.required()` forces it back.
#[test]
fn every_control_defaults_required_from_nullability() {
    for field in [
        Field::text(DummyUser::fields().name()),
        Field::choice(DummyUser::fields().name()),
        Field::file(DummyUser::fields().name()),
    ] {
        assert!(
            field.validate("").iter().any(|e| e.contains("is required")),
            "a non-nullable {field:?} refuses an empty submit"
        );
        let field = field.optional();
        assert!(field.validate("").is_empty(), "{field:?} opted out");
        let field = field.required();
        assert!(
            field.validate("").iter().any(|e| e.contains("is required")),
            "{field:?} opted back in"
        );
    }

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

/// A modifier on the wrong control is a declaration bug, named at the call.
#[test]
#[should_panic(expected = "`.email()` applies to a text field, and `name` is not one")]
fn a_text_modifier_on_a_choice_panics() {
    let _ = Field::choice(DummyUser::fields().name()).email();
}

#[test]
#[should_panic(expected = "`.options()` applies to a choice field, and `name` is not one")]
fn a_choice_modifier_on_a_text_field_panics() {
    let _ = Field::text(DummyUser::fields().name()).options(vec!["a".to_string()]);
}
