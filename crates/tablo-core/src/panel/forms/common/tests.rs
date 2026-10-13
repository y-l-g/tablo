use super::*;

/// One boolean vocabulary for framework form flags: `1` and `true` are truthy everywhere
/// (`confirm`, `clear_<field>`), and nothing else is.
#[test]
fn truthy_accepts_one_vocabulary() {
    assert!(truthy("1") && truthy("true"));
    assert!(!truthy("yes") && !truthy("") && !truthy("on") && !truthy("TRUE"));
}

#[test]
fn reject_unknown_form_keys_allows_declared_plus_csrf() {
    use crate::schema::{Field, Schema};

    #[derive(Debug, toasty::Model)]
    struct Member {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    let schema = Schema::new(Field::text(Member::fields().name()));

    let unclaimed = |posted: &[(&str, &str)]| {
        let posted: Vec<(String, String)> = posted
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        schema.read_unclaimed(&posted).unwrap().1
    };

    // Declared keys + csrf_token pass.
    let posted = unclaimed(&[("name", "Ada"), (crate::csrf::FIELD_NAME, "some-token")]);
    assert!(reject_unknown_form_keys(&schema, posted).is_ok());

    // Absent keys are fine: an edit completes them from the stored record.
    let posted = unclaimed(&[(crate::csrf::FIELD_NAME, "some-token")]);
    assert!(reject_unknown_form_keys(&schema, posted).is_ok());

    // role/tenant_id smuggling is a 400.
    let posted = unclaimed(&[("name", "Ada"), ("role", "admin"), ("tenant_id", "victim")]);
    assert!(reject_unknown_form_keys(&schema, posted).is_err());
}
