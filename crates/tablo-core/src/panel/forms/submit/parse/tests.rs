use super::*;
use crate::schema::Field;

#[test]
fn only_a_repeater_owns_the_errors_under_its_key() {
    let form = Schema::new(Field::text(crate::test_support::DummyUser::fields().name()));
    assert!(renders_under(&form, "name", "name"));
    assert!(
        !renders_under(&form, "name", "name.0.label"),
        "a text field posts no rows"
    );
    assert!(!renders_under(&form, "name", "names"));
}
