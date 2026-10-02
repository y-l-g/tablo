//! Applies presence, email, and typed-parse rules to a submitted string.

use email_address::{EmailAddress, Options};

use crate::form::{FieldError, FormScalar};

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

/// Converts a `datetime-local` value to the RFC 3339 string a timestamp parses, or `None` when the shape is not one the control sends.
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

/// Formats a stored timestamp as the `datetime-local` value its control renders, rendering anything else empty.
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

/// Reads a submitted string back as the stored spelling or the error message.
type ValueParser = fn(&str) -> Result<String, String>;

/// Binds the parser for scalar type `T`, rejecting what `T` refuses and storing what `T`'s own form spelling produces.
fn scalar_parser<T: FormScalar>(value: &str) -> Result<String, String> {
    T::parse_form(value).map(|parsed| parsed.to_form())
}

/// Holds the rules a field declares on top of presence and the wording of every message they produce.
#[derive(Clone, Copy, Default)]
pub(crate) struct Rules {
    email: bool,
    parser: Option<ValueParser>,
}

impl Rules {
    /// Holds a field with no declared rule: presence alone.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Adds the parse rule of the scalar type `T`.
    pub(crate) fn scalar<T: FormScalar>(mut self) -> Self {
        self.parser = Some(scalar_parser::<T>);
        self
    }

    /// Turns on the email rule.
    pub(crate) fn set_email(&mut self) {
        self.email = true;
    }

    /// Reports whether the email rule is on.
    pub(crate) fn is_email(&self) -> bool {
        self.email
    }

    /// Validates `value` in rule order and skips the email and typed-parse rules on an empty submit.
    pub(crate) fn validate(
        &self,
        key: &str,
        label: &str,
        required: bool,
        value: &str,
    ) -> Vec<FieldError> {
        let v = value.trim();
        let mut errs = Vec::new();
        if required && v.is_empty() {
            errs.push(FieldError::unanswered(key, required_error(label)));
        }
        if self.email && !v.is_empty() && !is_email(v) {
            errs.push(FieldError::invalid(
                key,
                format!("{label} must be a valid email"),
            ));
        }
        if !v.is_empty()
            && errs.is_empty()
            && let Some(parser) = &self.parser
            && let Err(message) = parser(v)
        {
            errs.push(FieldError::invalid(key, message));
        }
        errs
    }

    /// Returns the stored spelling of an already-validated submission and reports a failure rather than guessing.
    pub(crate) fn normalize(&self, value: &str) -> Result<String, String> {
        let v = value.trim();
        match &self.parser {
            Some(parser) => parser(v),
            None => Ok(v.to_string()),
        }
    }
}

/// Returns the message for an empty submit.
pub(crate) fn required_error(label: &str) -> String {
    format!("{label} is required")
}

/// Caps the longest address the rule accepts.
const EMAIL_MAX_LENGTH: usize = 254;

/// Reports whether `value` is an address the email rule accepts, requiring a TLD and refusing display text.
fn is_email(value: &str) -> bool {
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
