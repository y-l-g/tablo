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

/// The civil types read what their `date`, `time` and `datetime-local` controls send.
#[test]
fn a_civil_value_parses_its_controls_submission() {
    use jiff::civil::{Date, DateTime, Time, date, time};

    assert_eq!(Date::parse_form("2024-01-15"), Ok(date(2024, 1, 15)));
    assert_eq!(Time::parse_form("09:30"), Ok(time(9, 30, 0, 0)));
    assert_eq!(Time::parse_form("09:30:15"), Ok(time(9, 30, 15, 0)));
    assert_eq!(
        DateTime::parse_form("2024-01-15T09:30"),
        Ok(date(2024, 1, 15).at(9, 30, 0, 0))
    );
    assert_eq!(
        Date::parse_form("2024-02-30"),
        Err("`2024-02-30` is not a valid date".to_string())
    );
    assert_eq!(
        DateTime::parse_form("2024-01-15").map(|value| value.to_form()),
        Ok("2024-01-15T00:00:00".to_string()),
        "a bare date reads as midnight"
    );
    assert_eq!(Time::input_value("not a time"), "");
}
