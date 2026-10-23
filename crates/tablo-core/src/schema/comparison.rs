//! Comparisons: a text field whose value orders against another field's of the same form, as an
//! end date after a start date.
//!
//! The controls' check runs them, so a refused comparison renders in the same round as a missing
//! field, where `validate_record` waits until the whole struct parses.

use std::collections::HashSet;

use super::{
    Schema,
    fields::{Field, TextControl},
};
use crate::{
    DeclarationErrorKind, FieldRuleFault, Text, text,
    values::{FieldError, FormValues},
};

/// How a field's value relates to the compared field's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Relation {
    Gt,
    Gte,
    Lt,
    Lte,
    Different,
}

/// A field's comparison with the field posting under `other`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Comparison {
    pub(crate) relation: Relation,
    pub(crate) other: String,
}

/// The input types whose values order: a number, a date, a time, and a date and time.
const ORDERED_INPUTS: [&str; 4] = ["number", "date", "time", "datetime-local"];

impl Comparison {
    /// The sentence refusing `value` against `other`, both trimmed and non-empty, or `None` when
    /// they relate as declared or the field's type parses neither.
    fn refusal(
        &self,
        text: &TextControl,
        label: &Text,
        value: &str,
        other_label: &Text,
        other: &str,
    ) -> Option<Text> {
        let refused = |id: &'static str| text!(id, label = label, other = other_label);
        if self.relation == Relation::Different {
            let same = text.same_value(value, other);
            return same.then(|| refused("tablo-field-different"));
        }
        let order = text.order(value, other)?;
        let (holds, number, temporal) = match self.relation {
            Relation::Gt => (order.is_gt(), "tablo-field-gt", "tablo-field-after"),
            Relation::Gte => (order.is_ge(), "tablo-field-gte", "tablo-field-not-before"),
            Relation::Lt => (order.is_lt(), "tablo-field-lt", "tablo-field-before"),
            Relation::Lte | Relation::Different => {
                (order.is_le(), "tablo-field-lte", "tablo-field-not-after")
            }
        };
        let id = if text.input_type() == "number" {
            number
        } else {
            temporal
        };
        (!holds).then(|| refused(id))
    }
}

impl<F> Schema<F> {
    /// The refusal of `field`'s first comparison its posted value breaks, among `values`, the
    /// submission, against `stored`, the record's stored values. The compared field's value is
    /// its posted one, else its stored one; a blank value on either side, or a compared field the
    /// submission hides, skips the comparison. A pair the record stores unchanged on both sides
    /// is not checked again, as a value the record stores is not, but changing either side checks
    /// it.
    pub(crate) fn comparison_refusal(
        &self,
        field: &Field,
        values: &FormValues,
        stored: &FormValues,
        hidden: &HashSet<String>,
    ) -> Option<FieldError> {
        let text = field.as_text()?;
        let name = field.name();
        let value = values.text(name)?.trim();
        if value.is_empty() {
            return None;
        }
        for comparison in text.comparisons() {
            let key = comparison.other.as_str();
            if hidden.contains(key) {
                continue;
            }
            let posted = values.get(key);
            let unchanged = stored.get(name) == values.get(name)
                && posted.is_none_or(|posted| stored.get(key) == Some(posted));
            if unchanged {
                continue;
            }
            let other = values.text(key).or_else(|| stored.text(key)).map(str::trim);
            let Some(other) = other.filter(|other| !other.is_empty()) else {
                continue;
            };
            let other_label = self
                .fields
                .iter()
                .find(|field| field.name() == key)
                .map_or_else(|| Text::from(key), Field::label);
            if let Some(message) =
                comparison.refusal(text, &field.label(), value, &other_label, other)
            {
                return Some(FieldError::invalid(name, message));
            }
        }
        None
    }

    /// Each compared field's key, with the key of the field comparing with it: a refusal tells
    /// what the compared field holds, as a condition's watch does.
    pub(crate) fn comparison_watchers(&self) -> Vec<(String, Option<String>)> {
        let mut watchers = Vec::new();
        for field in &self.fields {
            let Some(text) = field.as_text() else {
                continue;
            };
            for comparison in text.comparisons() {
                watchers.push((comparison.other.clone(), Some(field.name().to_string())));
            }
        }
        watchers
    }

    /// What is wrong with each field's comparisons, reported under `prefix` and the field's key:
    /// a comparison with the field itself, a compared field the schema does not place, one that
    /// is no text field of the same type, and an ordering comparison over a type without an
    /// order.
    pub(crate) fn comparison_errors(&self, prefix: &str) -> Vec<DeclarationErrorKind> {
        let mut errors = Vec::new();
        for field in &self.fields {
            let Some(text) = field.as_text() else {
                continue;
            };
            for comparison in text.comparisons() {
                let other = comparison.other.clone();
                let fault = match self.fields.iter().find(|f| f.name() == other) {
                    _ if other == field.name() => FieldRuleFault::SelfComparison,
                    None => FieldRuleFault::UnplacedComparedField(other),
                    Some(compared)
                        if compared.as_text().is_none_or(|compared| {
                            compared.compared_as() != text.compared_as()
                        }) =>
                    {
                        FieldRuleFault::IncomparableField(other)
                    }
                    Some(_)
                        if comparison.relation != Relation::Different
                            && !ORDERED_INPUTS.contains(&text.input_type()) =>
                    {
                        FieldRuleFault::UnorderedComparison(other)
                    }
                    Some(_) => continue,
                };
                errors.push(DeclarationErrorKind::FieldRule {
                    field: format!("{prefix}{}", field.name()),
                    fault,
                });
            }
        }
        errors
    }
}
