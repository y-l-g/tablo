use crate::values::FormScalar;

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
            f64::parse_form(input).map_err(|error| error.english()),
            Err(rejected(input)),
            "f64 accepted {input}"
        );
        assert_eq!(
            f32::parse_form(input).map_err(|error| error.english()),
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

/// What a decimal type refuses whatever the type: anything but a plain number input's spelling.
#[cfg(any(feature = "rust_decimal", feature = "bigdecimal"))]
const NOT_PLAIN_DECIMALS: &[&str] = &[
    "abc", "1e3", "1E-7", "+1", "1_000", "1.", "--1", "-", ".", "1.2.3", "NaN", "inf", "0x10",
    "1,5", "١",
];

/// A decimal keeps the scale it was typed with and refuses what it cannot hold exactly.
#[cfg(feature = "rust_decimal")]
#[test]
fn a_rust_decimal_parses_strictly_and_keeps_its_scale() {
    use rust_decimal::Decimal;

    assert_eq!(
        Decimal::parse_form("12.50").map(|value| value.to_form()),
        Ok("12.50".to_string())
    );
    assert_eq!(
        Decimal::parse_form(".5").map(|value| value.to_form()),
        Ok("0.5".to_string())
    );
    assert_eq!(Decimal::parse_form("-3"), Ok(Decimal::from(-3)));
    let past_scale = format!("0.{}1", "0".repeat(28));
    let past_mantissa = "79228162514264337593543950336";
    for input in NOT_PLAIN_DECIMALS
        .iter()
        .copied()
        .chain([past_scale.as_str(), past_mantissa])
    {
        assert_eq!(
            Decimal::parse_form(input).map_err(|error| error.english()),
            Err(rejected(input)),
            "Decimal accepted {input}"
        );
    }
}

/// A big decimal keeps its scale, and its form spelling is the plain one its parse reads back,
/// not the exponent its `Display` writes.
#[cfg(feature = "bigdecimal")]
#[test]
fn a_big_decimal_parses_strictly_and_spells_plainly() {
    use bigdecimal::BigDecimal;

    assert_eq!(
        BigDecimal::parse_form("12.50").map(|value| value.to_form()),
        Ok("12.50".to_string())
    );
    let tiny: BigDecimal = "1E-7".parse().unwrap();
    assert_eq!(tiny.to_form(), "0.0000001");
    assert_eq!(BigDecimal::parse_form(&tiny.to_form()), Ok(tiny));
    for input in NOT_PLAIN_DECIMALS {
        assert_eq!(
            BigDecimal::parse_form(input).map_err(|error| error.english()),
            Err(rejected(input)),
            "BigDecimal accepted {input}"
        );
    }
}

#[cfg(feature = "rust_decimal")]
proptest::proptest! {
    /// A decimal's form spelling parses back to the same value at the same scale.
    #[test]
    fn a_rust_decimal_round_trips_through_its_form_spelling(
        mantissa in proptest::num::i64::ANY,
        scale in 0u32..=28,
    ) {
        let value = rust_decimal::Decimal::new(mantissa, scale);
        let read = rust_decimal::Decimal::parse_form(&value.to_form());
        proptest::prop_assert_eq!(read.map(|read| (read, read.scale())), Ok((value, scale)));
    }
}

#[cfg(feature = "bigdecimal")]
proptest::proptest! {
    /// A big decimal's form spelling parses back to the same value and spelling, whatever its
    /// scale, an exponent `Display` would write included.
    #[test]
    fn a_big_decimal_round_trips_through_its_form_spelling(
        mantissa in proptest::num::i128::ANY,
        scale in -40i64..=40,
    ) {
        let value = bigdecimal::BigDecimal::new(mantissa.into(), scale);
        let read = bigdecimal::BigDecimal::parse_form(&value.to_form()).expect("parses");
        proptest::prop_assert_eq!(read.to_form(), value.to_form());
        proptest::prop_assert_eq!(read, value);
    }
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
        Date::parse_form("2024-02-30").map_err(|error| error.english()),
        Err("`2024-02-30` is not a valid date".to_string())
    );
    assert_eq!(
        DateTime::parse_form("2024-01-15").map(|value| value.to_form()),
        Ok("2024-01-15T00:00:00".to_string()),
        "a bare date reads as midnight"
    );
    assert_eq!(
        Time::input_value("not a time", &jiff::tz::TimeZone::UTC),
        ""
    );
}

proptest::proptest! {
    /// A timestamp's control shows its instant as a wall-clock time in the zone, and reads that
    /// time back in the same zone as the instant truncated to the minute the control edits to.
    #[test]
    fn a_timestamp_reads_back_what_its_control_shows(
        seconds in -8_000_000_000i64..8_000_000_000,
        offset_minutes in -(14 * 60)..=(14 * 60),
    ) {
        use jiff::{Timestamp, tz::{Offset, TimeZone}};

        let zone = TimeZone::fixed(Offset::from_seconds(offset_minutes * 60).unwrap());
        let instant = Timestamp::from_second(seconds).unwrap();
        let shown = Timestamp::input_value(&instant.to_string(), &zone);
        let read = Timestamp::parse_form(&Timestamp::form_value(&shown, &zone));
        let minute = Timestamp::from_second(seconds.div_euclid(60) * 60).unwrap();
        proptest::prop_assert_eq!(read, Ok(minute));
    }
}

/// A wall-clock time names no instant until a zone reads it: the parse alone refuses it, and the
/// control's read places it in the zone. A value with an offset names its instant whatever the
/// zone.
#[test]
fn a_timestamp_reads_a_wall_clock_time_in_the_zone_only() {
    use jiff::{
        Timestamp,
        tz::{Offset, TimeZone},
    };

    let paris_summer = TimeZone::fixed(Offset::from_hours(2).unwrap());
    assert!(Timestamp::parse_form("2026-10-20T14:00").is_err());
    assert_eq!(
        Timestamp::form_value("2026-10-20T14:00", &paris_summer),
        "2026-10-20T12:00:00Z"
    );
    assert_eq!(
        Timestamp::form_value("2026-10-20T14:00:00-05:00", &paris_summer),
        "2026-10-20T19:00:00Z"
    );
    let annotated = "2026-10-20T14:00[America/New_York]";
    assert_eq!(Timestamp::form_value(annotated, &paris_summer), annotated);
    assert!(Timestamp::parse_form(annotated).is_err());
    assert_eq!(Timestamp::form_value("soon", &paris_summer), "soon");
}

/// A wall-clock time the zone skips moves forward by the gap, and one it repeats takes its
/// earlier instant.
#[test]
fn a_timestamp_reads_a_skipped_or_repeated_time_compatibly() {
    use jiff::{Timestamp, tz::TimeZone};

    let paris = TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
    assert_eq!(
        Timestamp::form_value("2026-03-29T02:30", &paris),
        "2026-03-29T01:30:00Z"
    );
    assert_eq!(
        Timestamp::form_value("2026-10-25T02:30", &paris),
        "2026-10-25T00:30:00Z"
    );
}

proptest::proptest! {
    /// Whatever the URL rule takes opens with an `http` or `https` scheme and holds no
    /// whitespace or control character, so a link rendering it can only navigate.
    #[test]
    fn a_url_the_rule_takes_is_an_http_link(
        value in "(http|HTTPS|https|ftp|javascript|data):(//)?[ -~\t\n\u{0}]{0,16}",
    ) {
        if super::is_web_url(&value) {
            let lower = value.to_ascii_lowercase();
            proptest::prop_assert!(lower.starts_with("http://") || lower.starts_with("https://"));
            proptest::prop_assert!(!value.chars().any(|c| c.is_whitespace() || c.is_control()));
        }
    }
}
