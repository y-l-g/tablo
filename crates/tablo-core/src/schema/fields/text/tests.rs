use jiff::tz::TimeZone;

use super::super::{Field, test_support::DummyUser};
use crate::values::FormValue;

#[test]
fn text_input_email_validates() {
    let input: Field = Field::text(DummyUser::fields().email()).email().into();
    assert!(
        input
            .check(&FormValue::from("not-an-email"), &TimeZone::UTC)
            .is_some(),
        "email should reject invalid"
    );
    assert!(
        input
            .check(&FormValue::from("a@"), &TimeZone::UTC)
            .is_some(),
        "email should reject partial"
    );
    assert!(
        input
            .check(&FormValue::from("a@b.com"), &TimeZone::UTC)
            .is_none(),
        "email should accept valid"
    );
    assert!(
        input
            .check(&FormValue::from(" a@b.com "), &TimeZone::UTC)
            .is_none(),
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
            input.check(&FormValue::from(ok), &TimeZone::UTC).is_none(),
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
            input
                .check(&FormValue::from(&bad), &TimeZone::UTC)
                .is_some(),
            "{bad} should fail"
        );
    }
}

/// The URL rule takes an absolute `http` or `https` URL naming a host, and refuses any other
/// scheme, `javascript:` first: a link to the stored value would run it.
#[test]
fn text_input_url_takes_only_web_urls() {
    let input: Field = Field::text(DummyUser::fields().name()).url().into();
    for ok in [
        "https://example.com",
        "http://example.com/a?b=c#d",
        " https://example.com/path ",
        "https://例え.jp/パス",
        "https://127.0.0.1:8080",
        "HTTPS://EXAMPLE.COM",
    ] {
        assert!(
            input.check(&FormValue::from(ok), &TimeZone::UTC).is_none(),
            "{ok} should pass"
        );
    }
    for bad in [
        "javascript:alert(1)",
        "JavaScript://example.com/%0Aalert(1)",
        "data:text/html,<script>alert(1)</script>",
        "mailto:a@example.com",
        "ftp://example.com",
        "example.com",
        "/relative/path",
        "https://",
        "https://exa mple.com",
    ] {
        assert!(
            input.check(&FormValue::from(bad), &TimeZone::UTC).is_some(),
            "{bad} should fail"
        );
    }
}
