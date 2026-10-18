use super::super::{Field, test_support::DummyUser};
use crate::values::FormValue;

#[test]
fn text_input_email_validates() {
    let input: Field = Field::text(DummyUser::fields().email()).email().into();
    assert!(
        input.check(&FormValue::from("not-an-email")).is_some(),
        "email should reject invalid"
    );
    assert!(
        input.check(&FormValue::from("a@")).is_some(),
        "email should reject partial"
    );
    assert!(
        input.check(&FormValue::from("a@b.com")).is_none(),
        "email should accept valid"
    );
    assert!(
        input.check(&FormValue::from(" a@b.com ")).is_none(),
        "email should trim"
    );
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
        assert!(
            input.check(&FormValue::from(ok)).is_none(),
            "{ok} should pass"
        );
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
        assert!(
            input.check(&FormValue::from(&bad)).is_some(),
            "{bad} should fail"
        );
    }
}
