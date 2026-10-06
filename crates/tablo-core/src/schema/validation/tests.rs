use crate::form::FormScalar;

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
        assert_eq!(
            f64::parse_form(input),
            Err(rejected(input)),
            "f64 accepted {input}"
        );
        assert_eq!(
            f32::parse_form(input),
            Err(rejected(input)),
            "f32 accepted {input}"
        );
    }
}

/// The same type keeps accepting a finite number, and spells it through
/// `f64`'s `Display`.
#[test]
fn a_float_accepts_and_spells_a_finite_number() {
    assert_eq!(f64::parse_form("-0.25"), Ok(-0.25));
    assert_eq!(f64::parse_form("1e3"), Ok(1000.0));
    assert_eq!(
        f64::parse_form("12.50").expect("12.50 parses").to_form(),
        "12.5"
    );
}
