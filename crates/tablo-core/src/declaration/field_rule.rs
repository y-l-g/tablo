//! The declaration mistakes of a field's rules, and their wording.

use std::fmt;

/// Why a field's declared rules cannot work:
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
    /// `accepted` on a control other than the built-in checkbox or switch.
    NotCheckbox,
    /// `min_items` or `max_items` on a choice that is not `multiple`.
    SingleValue,
    /// A multiple choice's `min_items` above the count of static options it offers, carried.
    FewerOptions(usize),
    /// A repeater's `max_items` above [`MAX_REPEATER_ROWS`](crate::MAX_REPEATER_ROWS),
    /// carried.
    AboveRowCap(usize),
    /// A field always disabled, with no default, whose blank answer breaks its own rules: an
    /// `accepted` checkbox or a `min_items`, so every create is refused.
    RefusedBlank,
    /// A comparison with a field the schema does not place, carried.
    ///
    /// Place the compared field in the same schema, or in the same repeater row.
    UnplacedComparedField(String),
    /// A comparison with a field that is no text field of the same type, carried.
    ///
    /// Compare two fields of one type: an `Option<T>` field compares with a `T` one.
    IncomparableField(String),
    /// An ordering comparison over a type without an order, carrying the compared field.
    ///
    /// Order a number, date, time or timestamp field, or compare with `different`.
    UnorderedComparison(String),
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
        FieldRuleFault::NotCheckbox => write!(
            f,
            "field '{field}' is `accepted` but is not the built-in checkbox or switch: declare \
             it with `Field::toggle` or `Field::toggle_input`"
        ),
        FieldRuleFault::SingleValue => write!(
            f,
            "field '{field}' bounds how many options it takes, but takes one: make it `multiple` \
             or drop `min_items` and `max_items`"
        ),
        FieldRuleFault::FewerOptions(options) => write!(
            f,
            "field '{field}' needs more options chosen than the {options} it offers, so it \
             refuses every value"
        ),
        FieldRuleFault::AboveRowCap(max) => write!(
            f,
            "field '{field}' takes up to {max} rows, more than any repeater takes \
             (`MAX_REPEATER_ROWS`): lower `max_items`"
        ),
        FieldRuleFault::RefusedBlank => write!(
            f,
            "field '{field}' is disabled and has no default, but its own rules refuse its blank \
             answer, so every create is refused: give it a default or drop the rule"
        ),
        FieldRuleFault::UnplacedComparedField(other) => write!(
            f,
            "field '{field}' compares with field '{other}', which the schema does not place: \
             place it in the same schema, or in the same repeater row"
        ),
        FieldRuleFault::IncomparableField(other) => write!(
            f,
            "field '{field}' compares with field '{other}', which is not a text field of the same \
             type: compare two fields of one type"
        ),
        FieldRuleFault::UnorderedComparison(other) => write!(
            f,
            "field '{field}' orders against field '{other}', but its type has no order: compare \
             a number, date, time or timestamp field, or use `different`"
        ),
    }
}
