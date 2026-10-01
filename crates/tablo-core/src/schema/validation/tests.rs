use super::Rules;

/// The messages `errors` carries, in the order the rules reported them.
fn messages(errors: &[crate::form::FieldError]) -> Vec<&str> {
    errors.iter().map(|error| error.message.as_str()).collect()
}

/// The typed rule words one message for a value the type cannot parse and
/// for one it parses into a value it does not accept.
fn rejected(input: &str) -> String {
    format!("`{input}` is not a valid number")
}

/// `f32`/`f64` `FromStr` accepts `NaN`, `inf` and `-inf`. They are refused like
/// any other unparseable submission.
#[test]
fn a_float_refuses_a_non_finite_parse() {
    for input in ["NaN", "nan", "inf", "-inf", "infinity", "1e400"] {
        let f64_errs = Rules::new()
            .scalar::<f64>()
            .validate("amount", "Amount", true, input);
        let f32_errs = Rules::new()
            .scalar::<f32>()
            .validate("amount", "Amount", true, input);
        assert_eq!(
            messages(&f64_errs),
            [rejected(input)],
            "f64 accepted {input}"
        );
        assert_eq!(
            messages(&f32_errs),
            [rejected(input)],
            "f32 accepted {input}"
        );
    }
}

/// The same field keeps accepting a finite number, and normalises it
/// through `f64`'s `Display` as any typed rule does.
#[test]
fn a_float_accepts_and_normalises_a_finite_number() {
    let rules = Rules::new().scalar::<f64>();
    assert!(rules.validate("amount", "Amount", true, "-0.25").is_empty());
    assert!(rules.validate("amount", "Amount", true, "1e3").is_empty());
    assert_eq!(
        rules.normalize(" 12.50 ").expect("12.50 parses"),
        "12.5",
        "a finite value stores its own spelling"
    );
}
