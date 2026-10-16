use super::Profile;
use crate::{FormInput, form::FormValues};

fn values(pairs: &[(&str, &str)]) -> FormValues {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

/// A blank or all-space password keeps the current one; a typed one keeps its outer spaces.
#[test]
fn a_profile_reads_blank_passwords_as_none() {
    let cx = crate::test_support::cx();
    let kept = Profile::parse(
        &cx,
        &values(&[
            ("name", "Ada"),
            ("email", "ada@example.com"),
            ("password", "   "),
            ("current_password", ""),
        ]),
    )
    .expect("a profile without passwords parses");
    assert_eq!(kept.password, None);
    assert_eq!(kept.current_password, None);
    assert!(kept.password_errors().is_empty());

    let changed = Profile::parse(
        &cx,
        &values(&[
            ("name", "Ada"),
            ("email", "ada@example.com"),
            ("password", " new pass "),
            ("password_confirmation", " new pass "),
            ("current_password", " old "),
        ]),
    )
    .expect("a profile with passwords parses");
    assert_eq!(changed.password.as_deref(), Some(" new pass "));
    assert_eq!(changed.current_password.as_deref(), Some(" old "));
    assert!(changed.password_errors().is_empty());
}

/// The name and email are required; the passwords are not.
#[test]
fn a_profile_requires_a_name_and_an_email() {
    let cx = crate::test_support::cx();
    let refused = Profile::parse(&cx, &values(&[("name", " ")])).expect_err("refused");
    let mut keys: Vec<&str> = refused.iter().map(|error| error.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["email", "name"]);
}
