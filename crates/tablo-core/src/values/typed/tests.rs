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
