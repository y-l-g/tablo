//! The typed values a text field parses, and the email and URL rules.

use std::cmp::Ordering;

use email_address::{EmailAddress, Options};
use jiff::tz::TimeZone;

/// Parses a typed column's text submission and names the type in the error it produces.
///
/// A text control over a typed column renders the stored value in the input its type names, and
/// a value the type refuses is an inline error naming it: `` `twelve` is not a valid whole
/// number ``. The built-in types:
///
/// | Column type | Input |
/// | --- | --- |
/// | an integer | `type="number"` |
/// | `f32`, `f64` | `type="number" step="any"` |
/// | `jiff::civil::Date` | `type="date"` |
/// | `jiff::civil::Time` | `type="time"`, to the minute |
/// | `jiff::civil::DateTime` | `type="datetime-local"`, to the minute |
/// | `jiff::Timestamp` | `type="datetime-local"`, to the minute, in the request's time zone |
/// | `bool`, `Uuid`, `TenantId` | `type="text"` |
///
/// A `jiff::Zoned` column has no control: no browser input carries a zone.
pub trait TypedValue: std::fmt::Display + std::str::FromStr {
    /// What this type is called in a validation error.
    const NOUN: &'static str;

    /// The `type` attribute of the text control that edits it.
    const INPUT_TYPE: &'static str = "text";

    /// The control's `step` attribute, when its input type takes one.
    const STEP: Option<&'static str> = None;

    /// Reads a trimmed, non-empty submission, or `None` when the type refuses it.
    fn parse_input(value: &str) -> Option<Self> {
        value.parse().ok()
    }

    /// The control's `value` for a stored value's form spelling, shown in the request's time zone
    /// `zone`: the spelling itself by default.
    fn input_value(stored: &str, zone: &TimeZone) -> String {
        let _ = zone;
        stored.to_string()
    }

    /// The form spelling of what the control posted, read in the request's time zone `zone`: the
    /// posted text itself by default. A type whose control posts a wall-clock time, as a
    /// timestamp's does, reads it here, so every later step parses one spelling.
    fn form_value(posted: &str, zone: &TimeZone) -> String {
        let _ = zone;
        posted.to_string()
    }

    /// How `self` orders against `other`, which a text field's
    /// [`min`](crate::TextField::min) and [`max`](crate::TextField::max) compare by. `None` by
    /// default: a type without an order takes no bound.
    fn order(&self, _other: &Self) -> Option<Ordering> {
        None
    }
}

/// Orders by `PartialOrd`, for a type whose order is its value's.
macro_rules! ordered_by_value {
    () => {
        fn order(&self, other: &Self) -> Option<Ordering> {
            self.partial_cmp(other)
        }
    };
}

/// Binds the integer types a typed field accepts.
macro_rules! typed_whole_number {
    ($($ty:ty),* $(,)?) => {
        $(
            impl TypedValue for $ty {
                const NOUN: &'static str = "whole number";
                const INPUT_TYPE: &'static str = "number";
                ordered_by_value!();
            }
        )*
    };
}

typed_whole_number!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

impl TypedValue for bool {
    const NOUN: &'static str = "yes/no value";
}

/// Accepts finite values only.
impl TypedValue for f32 {
    const NOUN: &'static str = "number";
    const INPUT_TYPE: &'static str = "number";
    // The default step of 1 makes a browser refuse a fraction.
    const STEP: Option<&'static str> = Some("any");

    fn parse_input(value: &str) -> Option<Self> {
        value
            .parse::<f32>()
            .ok()
            .filter(|parsed| parsed.is_finite())
    }

    ordered_by_value!();
}

impl TypedValue for f64 {
    const NOUN: &'static str = "number";
    const INPUT_TYPE: &'static str = "number";
    // The default step of 1 makes a browser refuse a fraction.
    const STEP: Option<&'static str> = Some("any");

    fn parse_input(value: &str) -> Option<Self> {
        value
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite())
    }

    ordered_by_value!();
}

impl TypedValue for uuid::Uuid {
    const NOUN: &'static str = "identifier";
}

impl TypedValue for crate::TenantId {
    const NOUN: &'static str = "identifier";
}

/// Binds a timestamp to a `datetime-local` control, which shows and reads the wall-clock time in
/// the request's [`time_zone`](crate::time_zone).
impl TypedValue for jiff::Timestamp {
    const NOUN: &'static str = "timestamp";
    const INPUT_TYPE: &'static str = "datetime-local";

    /// Parses RFC 3339 only: a wall-clock time names no instant until
    /// [`form_value`](TypedValue::form_value) reads it in a zone.
    fn parse_input(value: &str) -> Option<Self> {
        value.parse().ok()
    }

    fn input_value(stored: &str, zone: &TimeZone) -> String {
        format_timestamp_input(stored, zone)
    }

    /// An instant spelled with an offset comes back in RFC 3339 UTC, and a `datetime-local` value
    /// is read in `zone`; anything else is kept as posted, for the parse to refuse.
    fn form_value(posted: &str, zone: &TimeZone) -> String {
        posted
            .trim()
            .parse::<jiff::Timestamp>()
            .ok()
            .or_else(|| read_datetime_local(posted, zone))
            .map_or_else(|| posted.to_string(), |instant| instant.to_string())
    }

    ordered_by_value!();
}

/// Binds a calendar date to a `date` control.
impl TypedValue for jiff::civil::Date {
    const NOUN: &'static str = "date";
    const INPUT_TYPE: &'static str = "date";

    ordered_by_value!();
}

/// Binds a wall-clock time to a `time` control, which edits it to the minute.
impl TypedValue for jiff::civil::Time {
    const NOUN: &'static str = "time";
    const INPUT_TYPE: &'static str = "time";

    fn input_value(stored: &str, _zone: &TimeZone) -> String {
        civil_input(stored, |time: jiff::civil::Time| {
            time.strftime("%H:%M").to_string()
        })
    }

    ordered_by_value!();
}

/// Binds a date and wall-clock time without a zone to a `datetime-local` control, which edits it to
/// the minute.
impl TypedValue for jiff::civil::DateTime {
    const NOUN: &'static str = "date and time";
    const INPUT_TYPE: &'static str = "datetime-local";

    fn input_value(stored: &str, _zone: &TimeZone) -> String {
        civil_input(stored, |datetime: jiff::civil::DateTime| {
            datetime.strftime("%Y-%m-%dT%H:%M").to_string()
        })
    }

    ordered_by_value!();
}

/// Formats a stored civil value for its control, rendering anything that does not parse empty.
fn civil_input<T: std::str::FromStr>(stored: &str, format: impl Fn(T) -> String) -> String {
    stored.trim().parse().map(format).unwrap_or_default()
}

/// The instant a `datetime-local` value names as a wall-clock time in `zone`, or `None` when the
/// value is not a date and time, or carries an offset or a zone annotation: jiff's civil parse
/// would drop either. A time the zone skips or repeats resolves as jiff's compatible
/// disambiguation does: a skipped time moves forward by the gap, a repeated one takes its earlier
/// instant.
pub(crate) fn read_datetime_local(value: &str, zone: &TimeZone) -> Option<jiff::Timestamp> {
    let value = value.trim();
    if !value.contains(['T', 't'])
        || value.contains('[')
        || value.parse::<jiff::Timestamp>().is_ok()
    {
        return None;
    }
    let wall = value.parse::<jiff::civil::DateTime>().ok()?;
    zone.to_ambiguous_timestamp(wall).compatible().ok()
}

/// Formats a stored timestamp as the `datetime-local` value its control renders in `zone`,
/// rendering anything else empty.
fn format_timestamp_input(storage: &str, zone: &TimeZone) -> String {
    match storage.trim().parse::<jiff::Timestamp>() {
        Ok(parsed) => parsed
            .to_zoned(zone.clone())
            .strftime("%Y-%m-%dT%H:%M")
            .to_string(),
        Err(_) => String::new(),
    }
}

/// Caps the longest address the rule accepts.
const EMAIL_MAX_LENGTH: usize = 254;

/// Reports whether `value` is an address the email rule accepts, requiring a TLD and refusing
/// display text.
pub(crate) fn is_email(value: &str) -> bool {
    value.len() <= EMAIL_MAX_LENGTH
        && EmailAddress::parse_with_options(
            value,
            Options::default()
                .with_required_tld()
                .without_display_text(),
        )
        .is_ok()
}

/// Reports whether `value` is an absolute `http` or `https` URL naming a host, as the URL rule
/// accepts. A browser's `type="url"` takes any scheme, `javascript:` included, which a link to the
/// stored value would run. The value must spell its `://` and hold no whitespace or control
/// character: the parser forgives `http:host` and strips a tab, which would store a value other
/// than the one it checked.
pub(crate) fn is_web_url(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once("://") else {
        return false;
    };
    (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        && !rest.is_empty()
        && !value.chars().any(|c| c.is_whitespace() || c.is_control())
        && url::Url::parse(value).is_ok_and(|url| url.host().is_some())
}

#[cfg(test)]
mod tests;
