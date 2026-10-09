use std::collections::HashMap;

use super::{SignUp, new_password_errors};
use crate::ActionInput;

fn values(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

/// The length rule counts characters, not bytes: eight accented letters pass, seven ASCII fail.
#[test]
fn a_new_password_needs_eight_characters_and_a_matching_confirmation() {
    assert!(new_password_errors("éééééééé", "éééééééé").is_empty());
    let errors = new_password_errors("1234567", "1234567");
    assert!(errors.contains_key("password"));
    assert!(!errors.contains_key("password_confirmation"));
    let errors = new_password_errors("12345678", "12345679");
    assert!(!errors.contains_key("password"));
    assert!(errors.contains_key("password_confirmation"));
}

/// The name and email are trimmed like any text field; the passwords keep every character.
#[test]
fn sign_up_parses_its_passwords_untrimmed() {
    let cx = crate::test_support::cx();
    let parsed = SignUp::parse(
        &cx,
        &values(&[
            ("name", " Ada "),
            ("email", "ada@example.com"),
            ("password", " pass word "),
            ("password_confirmation", " pass word "),
        ]),
    )
    .expect("a complete sign-up parses");
    assert_eq!(parsed.name, "Ada");
    assert_eq!(parsed.password, " pass word ");
    assert_eq!(parsed.password_confirmation, " pass word ");
}

/// Each missing key is refused once, and a blank password is missing.
#[test]
fn sign_up_refuses_each_missing_field() {
    let cx = crate::test_support::cx();
    let refused = SignUp::parse(
        &cx,
        &values(&[("email", "ada@example.com"), ("password", "  ")]),
    )
    .expect_err("an incomplete sign-up is refused");
    let mut keys: Vec<&str> = refused.iter().map(|error| error.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["name", "password", "password_confirmation"]);
}
