//! The declaration mistakes of a text field's rules, and their wording.

use std::fmt;

/// Why a text field's declared rules cannot work:
/// [`DeclarationErrorKind::FieldRule`](super::DeclarationErrorKind::FieldRule).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldRuleFault {
    /// A `min` or `max` that is no value the field's type parses, or a type without an order.
    ///
    /// Bound a number, date, time or timestamp field by a value of its type.
    UnorderedBound(String),
    /// A `pattern` the server does not compile, with the compiler's reason.
    ///
    /// Fix the pattern.
    InvalidPattern(String),
    /// A `min_length` above the `max_length`, or a `min` above the `max`: no value passes.
    ///
    /// Lower the minimum or raise the maximum.
    EmptyRange,
    /// A default that breaks a rule, so a create that keeps it is refused.
    ///
    /// Change the default or the rule.
    RefusedDefault(String),
    /// A default the field's type does not parse, as a timestamp spelled without an offset, so a
    /// create that keeps it is refused.
    ///
    /// Spell the default as the type does, or pass a value of the type.
    UnparsedDefault(String),
    /// A rich text editor over a field whose type is no text, as a number or a timestamp,
    /// carrying the input type its text control had.
    NotText(&'static str),
}

/// [`DeclarationErrorKind::FieldRule`](super::DeclarationErrorKind::FieldRule)'s sentence.
pub(super) fn fmt_field_rule(
    f: &mut fmt::Formatter<'_>,
    field: &str,
    fault: &FieldRuleFault,
) -> fmt::Result {
    match fault {
        FieldRuleFault::UnorderedBound(bound) => write!(
            f,
            "field '{field}' is bounded by '{bound}', which is no value of a type that orders: \
             bound a number, date, time or timestamp field by a value of its type"
        ),
        FieldRuleFault::InvalidPattern(error) => write!(
            f,
            "field '{field}' has a pattern that does not compile: {error}"
        ),
        FieldRuleFault::EmptyRange => write!(
            f,
            "field '{field}' declares a minimum above its maximum, so it refuses every value"
        ),
        FieldRuleFault::RefusedDefault(value) => write!(
            f,
            "field '{field}' defaults to '{value}', which its own rules refuse: change the default \
             or the rule"
        ),
        FieldRuleFault::UnparsedDefault(value) => write!(
            f,
            "field '{field}' defaults to '{value}', which is no value of its type: spell the \
             default as the type does, or pass a value of the type"
        ),
        FieldRuleFault::NotText(input) => write!(
            f,
            "field '{field}' is a rich text editor over a field its form edits as '{input}' \
             input, but rich text stores HTML: make the field a `String` or `Option<String>`"
        ),
    }
}
