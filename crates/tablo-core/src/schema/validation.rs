//! The typed values a text field parses, and the email rule.

use email_address::{EmailAddress, Options};

/// Parses a typed column's text submission and names the type in the error it produces.
pub trait TypedValue: std::fmt::Display + std::str::FromStr {
    /// What this type is called in a validation error.
    const NOUN: &'static str;

    /// The `type` attribute of the text control that edits it.
    const INPUT_TYPE: &'static str = "text";

    /// Reads a trimmed, non-empty submission, or `None` when the type refuses it.
    fn parse_input(value: &str) -> Option<Self> {
        value.parse().ok()
    }
}

/// Binds the integer types a typed field accepts.
macro_rules! typed_whole_number {
    ($($ty:ty),* $(,)?) => {
        $(
            impl TypedValue for $ty {
                const NOUN: &'static str = "whole number";
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

    fn parse_input(value: &str) -> Option<Self> {
        value
            .parse::<f32>()
            .ok()
            .filter(|parsed| parsed.is_finite())
    }
}

impl TypedValue for f64 {
    const NOUN: &'static str = "number";

    fn parse_input(value: &str) -> Option<Self> {
        value
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite())
    }
}

impl TypedValue for uuid::Uuid {
    const NOUN: &'static str = "identifier";
}

impl TypedValue for crate::TenantId {
    const NOUN: &'static str = "identifier";
}

/// Binds a timestamp to a `datetime-local` control read back as UTC.
impl TypedValue for jiff::Timestamp {
    const NOUN: &'static str = "timestamp";
    const INPUT_TYPE: &'static str = "datetime-local";

    /// Parses RFC 3339 plus the `datetime-local` shapes a browser sends, read as UTC.
    fn parse_input(value: &str) -> Option<Self> {
        if let Ok(parsed) = value.parse::<jiff::Timestamp>() {
            return Some(parsed);
        }
        normalize_datetime_local(value)?.parse().ok()
    }
}

/// Converts a `datetime-local` value to the RFC 3339 string a timestamp parses, or `None` when the
/// shape is not one the control sends.
fn normalize_datetime_local(value: &str) -> Option<String> {
    let t = value.find('T')?;
    let after_t = &value[t + 1..];
    if after_t.contains('+') || after_t.contains('-') {
        return None;
    }
    if value.ends_with(['Z', 'z']) {
        return None;
    }
    if value.len() == 16 {
        Some(format!("{value}:00Z"))
    } else {
        Some(format!("{value}Z"))
    }
}

/// Formats a stored timestamp as the `datetime-local` value its control renders, rendering anything
/// else empty.
pub(crate) fn format_timestamp_input(storage: &str) -> String {
    let trimmed = storage.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match trimmed.parse::<jiff::Timestamp>() {
        Ok(parsed) => parsed.strftime("%Y-%m-%dT%H:%M").to_string(),
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

#[cfg(test)]
mod tests;
