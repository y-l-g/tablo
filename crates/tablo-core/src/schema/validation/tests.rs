use super::Rules;

/// The typed rule words one message for a value the type cannot parse and
/// for one it parses into a value it does not accept.
fn rejected(input: &str) -> String {
    format!("`{input}` is not a valid number")
}

/// GH #297: `f32`/`f64` `FromStr` accepts `NaN`, `inf` and `-inf`, and the
/// typed rule passed them through as stored values. They are refused like
/// any other unparseable submission.
#[test]
fn a_float_refuses_a_non_finite_parse() {
    for input in ["NaN", "nan", "inf", "-inf", "infinity", "1e400"] {
        let f64_errs = Rules::new().typed::<f64>().validate("Amount", true, input);
        let f32_errs = Rules::new().typed::<f32>().validate("Amount", true, input);
        assert_eq!(f64_errs, vec![rejected(input)], "f64 accepted {input}");
        assert_eq!(f32_errs, vec![rejected(input)], "f32 accepted {input}");
    }
}

/// The same field keeps accepting a finite number, and normalises it
/// through `f64`'s `Display` as any typed rule does.
#[test]
fn a_float_accepts_and_normalises_a_finite_number() {
    let rules = Rules::new().typed::<f64>();
    assert!(rules.validate("Amount", true, "-0.25").is_empty());
    assert!(rules.validate("Amount", true, "1e3").is_empty());
    assert_eq!(
        rules.normalize(" 12.50 ").expect("12.50 parses"),
        "12.5",
        "a finite value stores its own spelling"
    );
}
