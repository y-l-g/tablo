//! The rules a field applies to a submitted string, and their messages.
//!
//! Presence, email and the typed parse are the rules a form applies without a
//! `Cx`. Every [`Field`](super::Field) reads its errors from one [`Rules`], and
//! the repeater walk in `Schema::validate` words its own required error with
//! [`required_error`]. The rules that need a `Cx` — `check_unique` in
//! `panel::forms` and a choice field's option-existence probe — stay with
//! their callers.

use email_address::{EmailAddress, Options};

use crate::form::{FieldError, FormScalar};

/// A typed column's own spelling rules.
///
/// The form edge is text: a control submits a `String`, so a column that is not
/// a `String` needs a `Display` to render and a parse to read back. `NOUN`
/// names the type in the error a user sees (`` `2024-13-01` is not a valid
/// timestamp ``), because "invalid" alone does not tell them what was expected.
///
/// Implemented for the types a panel binds rather than as a blanket over
/// `FromStr`: a blanket would let a field declare a parse only to have no
/// sensible message for it. An app type implements it to bind as a text field
/// and a record-form scalar ([`FormScalar`](crate::FormScalar)).
pub trait TypedValue: std::fmt::Display + std::str::FromStr {
    /// What this type is called in a validation error.
    const NOUN: &'static str;

    /// The `type` attribute of the text control that edits it.
    const INPUT_TYPE: &'static str = "text";

    /// Read a trimmed, non-empty submission, or `None` when the type refuses
    /// it.
    ///
    /// The default is `FromStr`. A type overrides it where `FromStr` accepts a
    /// value no user typed (`f64` parses `NaN`) or refuses one its control
    /// sends (a `datetime-local` value carries no zone).
    fn parse_input(value: &str) -> Option<Self> {
        value.parse().ok()
    }
}

/// The integer types a typed field binds: the whole family, so a derived
/// embedded value can hold any of them.
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

/// `FromStr` parses `NaN`, `inf`, `-inf`, and an overflowing literal (`1e400`)
/// into a float no field can hand back, so a float accepts finite values only.
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

/// A timestamp renders `type="datetime-local"`, whose value carries no zone:
/// the stored instant renders in UTC, and a submission is read back as UTC.
impl TypedValue for jiff::Timestamp {
    const NOUN: &'static str = "timestamp";
    const INPUT_TYPE: &'static str = "datetime-local";

    /// RFC 3339, plus the `datetime-local` shapes a browser sends
    /// (`YYYY-MM-DDTHH:MM`, with optional seconds and fraction), read as UTC.
    fn parse_input(value: &str) -> Option<Self> {
        if let Ok(parsed) = value.parse::<jiff::Timestamp>() {
            return Some(parsed);
        }
        normalize_datetime_local(value)?.parse().ok()
    }
}

/// A `datetime-local` value as the RFC 3339 string a timestamp parses, or
/// `None` when the shape is not one the control sends.
fn normalize_datetime_local(value: &str) -> Option<String> {
    let t = value.find('T')?;
    let after_t = &value[t + 1..];
    // A zone offset after the `T` means the value already names its offset;
    // the direct parse refused it, so it is not a valid timestamp. A valid
    // control value carries neither `+` nor `-` after the `T`.
    if after_t.contains('+') || after_t.contains('-') {
        return None;
    }
    if value.ends_with(['Z', 'z']) {
        return None;
    }
    // `YYYY-MM-DDTHH:MM` needs seconds before the zone; longer shapes carry
    // their own seconds and fraction.
    if value.len() == 16 {
        Some(format!("{value}:00Z"))
    } else {
        Some(format!("{value}Z"))
    }
}

/// A stored timestamp as the `datetime-local` value its control renders.
///
/// The control carries no zone, so the instant renders in UTC truncated to the
/// minute. Anything that is not a timestamp (empty, free text) renders empty.
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

/// How a field reads a submitted string back: the stored spelling, or the
/// error message.
type ValueParser = fn(&str) -> Result<String, String>;

/// The parser a scalar field binds: reject what `T` refuses and store what
/// `T`'s own form spelling produces for it.
///
/// Normalising through the spelling is the point, not a side effect: it is
/// what makes an edit that never touched the field write back a value of the
/// same shape it read, rather than an unreviewed re-spelling. A
/// `jiff::Timestamp` submitted as `2024-01-02T03:04` is stored as that
/// type's canonical RFC 3339 form. A `String` stores what was typed.
fn scalar_parser<T: FormScalar>(value: &str) -> Result<String, String> {
    T::parse_form(value).map(|parsed| parsed.to_form())
}

/// The rules a field declares on top of presence, and the wording of every
/// message they produce.
///
/// Presence is not one of them: whether an empty submit is refused is a
/// declaration on the field — a non-nullable column is required, a unique one
/// is never empty — so [`Rules::validate`] takes the caller's
/// resolved flag and a field with no other rule holds nothing at all.
#[derive(Clone, Copy, Default)]
pub(crate) struct Rules {
    email: bool,
    parser: Option<ValueParser>,
}

impl Rules {
    /// A field with no declared rule: presence alone.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Add the parse rule of the scalar type `T`.
    pub(crate) fn scalar<T: FormScalar>(mut self) -> Self {
        self.parser = Some(scalar_parser::<T>);
        self
    }

    /// Turn on the email rule.
    pub(crate) fn set_email(&mut self) {
        self.email = true;
    }

    /// Whether the email rule is on — the control's `type` attribute reads it.
    pub(crate) fn is_email(&self) -> bool {
        self.email
    }

    /// Validate `value`, in rule order: presence, email, typed parse.
    ///
    /// `required` is the caller's resolved presence flag. An empty submit is
    /// the presence rule's business alone: the email and parse rules skip it,
    /// so an optional field accepts empty whatever else it declares.
    ///
    /// `key` is the field's own key, `label` the wording its message uses.
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
        // The typed rule runs last and only on a value the rules
        // above accepted.
        if !v.is_empty()
            && errs.is_empty()
            && let Some(parser) = &self.parser
            && let Err(message) = parser(v)
        {
            errs.push(FieldError::invalid(key, message));
        }
        errs
    }

    /// The stored spelling of a submission the caller has already validated.
    ///
    /// The parse's form spelling for a scalar field, the trimmed submission
    /// otherwise — so a value the user left alone is written back in
    /// the shape the record fn wrote it, not in whichever spelling the browser
    /// sent. Callers that have not validated must not use this: it reports a
    /// failure rather than guessing.
    pub(crate) fn normalize(&self, value: &str) -> Result<String, String> {
        let v = value.trim();
        match &self.parser {
            Some(parser) => parser(v),
            None => Ok(v.to_string()),
        }
    }
}

/// The message for an empty submit, shared with the repeater walk in
/// `Schema::validate`.
pub(crate) fn required_error(label: &str) -> String {
    format!("{label} is required")
}

/// The longest address the rule accepts. RFC 5321 §4.5.3.1.3 carries 256
/// octets including the angle brackets, and `email_address` bounds the local
/// part and the domain separately rather than their sum.
const EMAIL_MAX_LENGTH: usize = 254;

/// Whether `value` is an address the email rule accepts.
///
/// `email_address` parses the RFC 5322 grammar. `with_required_tld` gives a
/// text domain two labels, so `a@b` and `a@b..c` are refused; a bracketed
/// literal takes the crate's other domain path and passes whatever its label
/// count, so `a@[127.0.0.1]` and `a@[IPv6:::1]` are accepted.
/// `without_display_text` refuses `Ada <ada@example.com>`, a header rather
/// than an address.
///
/// The rest of the accepted set is the crate's:
///
/// - a quoted local part, `"a b"@example.com`;
/// - a unicode local part or domain, `用户@例え.jp`;
/// - an unquoted local part refuses `(`, `)`, `,`, `:`, `;`, `<`, `>`, `[`, `]`, `\`, `"` and
///   space, so `a,b@b.com`, `a(b@b.com` and `a:b@b.com` are refused;
/// - a domain label starts and ends with a letter or digit, so `user@my_host.com` is accepted and
///   `a@b!.com` is refused;
/// - a single-character TLD, `a@b.c`.
///
/// The crate bounds the local part at 64 octets and the domain at 254;
/// [`EMAIL_MAX_LENGTH`] caps their sum.
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
