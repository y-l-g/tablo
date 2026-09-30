use super::*;

/// One boolean vocabulary for framework form flags: `1` and
/// `true` are truthy everywhere (`confirm`, `clear_<field>`); `yes` was a
/// delete-only extra and is gone.
#[test]
fn truthy_accepts_one_vocabulary() {
    assert!(truthy("1") && truthy("true"));
    assert!(!truthy("yes") && !truthy("") && !truthy("on") && !truthy("TRUE"));
}

#[test]
fn reject_unknown_form_keys_allows_declared_plus_csrf() {
    use crate::schema::{Schema, TextInput};

    #[derive(Debug, toasty::Model)]
    struct Member {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    let schema = Schema::new(TextInput::r#for(Member::fields().name()));

    // Declared keys + csrf_token pass.
    let values = HashMap::from([
        ("name".to_string(), "Ada".to_string()),
        (
            crate::csrf::FIELD_NAME.to_string(),
            "some-token".to_string(),
        ),
    ]);
    assert!(reject_unknown_form_keys(&schema, &values).is_ok());

    // Absent keys are fine: an edit completes them from the stored record.
    let values = HashMap::from([(
        crate::csrf::FIELD_NAME.to_string(),
        "some-token".to_string(),
    )]);
    assert!(reject_unknown_form_keys(&schema, &values).is_ok());

    // role/tenant_id smuggling is a 400.
    let values = HashMap::from([
        ("name".to_string(), "Ada".to_string()),
        ("role".to_string(), "admin".to_string()),
        ("tenant_id".to_string(), "victim".to_string()),
    ]);
    assert!(reject_unknown_form_keys(&schema, &values).is_err());
}
