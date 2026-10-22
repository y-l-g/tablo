//! What a field's own rules refuse in a submission, and what is wrong with the rules it declares.

use jiff::tz::TimeZone;

use super::{ControlKind, Field};
use crate::{
    DeclarationErrorKind, FieldRuleFault,
    schema::MAX_REPEATER_ROWS,
    values::{FieldError, FormValue, is_email, is_http_url},
};

/// The fewest and most items a list control takes: a multiple choice's chosen options, or a
/// repeater's rows. Both unset by default.
#[derive(Clone, Copy, Default)]
pub(crate) struct Items {
    pub(crate) min: Option<usize>,
    pub(crate) max: Option<usize>,
}

impl Items {
    fn is_set(self) -> bool {
        self.min.is_some() || self.max.is_some()
    }

    /// The sentence refusing `count` items of the field `label`, each one `noun`.
    fn refusal(self, label: &str, count: usize, noun: &str) -> Option<String> {
        let counted = |n: usize| format!("{n} {noun}{}", if n == 1 { "" } else { "s" });
        if let Some(min) = self.min
            && count < min
        {
            return Some(format!("{label} needs at least {}", counted(min)));
        }
        if let Some(max) = self.max
            && count > max
        {
            return Some(format!("{label} takes at most {}", counted(max)));
        }
        None
    }

    /// A minimum above `most`, the declared maximum or the most items the control can post.
    fn empty_range(self, most: Option<usize>) -> Option<FieldRuleFault> {
        let most = [self.max, most].into_iter().flatten().min()?;
        self.min
            .is_some_and(|min| min > most)
            .then_some(FieldRuleFault::EmptyRange)
    }
}

impl Field {
    /// What the control posts when it is left blank, for a control whose rules can refuse that:
    /// an unchecked checkbox, an empty multiple choice, a repeater without rows.
    fn blank(&self) -> Option<FormValue> {
        match &self.control {
            _ if self.checkbox => Some(FormValue::Text("false".to_string())),
            ControlKind::Choice(choice) if choice.is_multiple() => {
                Some(FormValue::List(Vec::new()))
            }
            ControlKind::Repeater(_) => Some(FormValue::Rows(Vec::new())),
            _ => None,
        }
    }

    /// Refuses a submission the control's own rules refuse: an unchecked
    /// [`accepted`](super::CustomField::accepted) checkbox, a multiple choice's or a repeater's
    /// count of items, and, for a non-empty text, an email or URL field's value, a text field's
    /// declared length, bounds and pattern, its bounds spelled as the control shows them in
    /// `zone`, and a rich text field's length.
    pub(crate) fn check(&self, value: &FormValue, zone: &TimeZone) -> Option<FieldError> {
        let label = self.label_str();
        let message = match (&self.control, value) {
            (_, FormValue::Text(posted)) if self.accepted && posted.trim() != "true" => {
                format!("{label} must be accepted")
            }
            (ControlKind::Choice(choice), FormValue::List(chosen)) => {
                choice.bounds.refusal(label, chosen.len(), "option")?
            }
            (ControlKind::Repeater(repeater), FormValue::Rows(rows)) => {
                repeater.bounds.refusal(label, rows.len(), "item")?
            }
            (control, FormValue::Text(posted)) => {
                let value = posted.trim();
                if value.is_empty() {
                    return None;
                }
                match control {
                    ControlKind::Text(text) if text.email && !is_email(value) => {
                        format!("{label} must be a valid email")
                    }
                    ControlKind::Text(text) if text.url && !is_http_url(value) => {
                        format!("{label} must be a valid http or https URL")
                    }
                    ControlKind::Text(text) => text.refusal(label, value, zone)?,
                    ControlKind::RichText(rich) => rich.refusal(label, value)?,
                    _ => return None,
                }
            }
            _ => return None,
        };
        Some(FieldError::invalid(self.name(), message))
    }

    /// Refuses the blank answer of a control the submission does not post, when its rules
    /// refuse it: a browser always posts one, so only a hand-made request leaves it out.
    pub(crate) fn check_unposted(&self, zone: &TimeZone) -> Option<FieldError> {
        self.check(&self.blank()?, zone)
    }

    /// What is wrong with the rules the field declares, reported under `field`: one that cannot
    /// work, a default its type does not parse or that breaks one, or a blank answer that breaks
    /// one on a field that is always disabled and has no default, which no create then passes.
    pub(crate) fn rule_errors(&self, field: &str) -> Vec<DeclarationErrorKind> {
        let mut faults = match &self.control {
            ControlKind::Text(text) => text.rule_faults(),
            ControlKind::RichText(rich) => rich.rule_faults(),
            ControlKind::Choice(choice) if !choice.is_multiple() => choice
                .bounds
                .is_set()
                .then_some(FieldRuleFault::SingleValue)
                .into_iter()
                .collect(),
            ControlKind::Choice(choice) => {
                let offered = (!choice.is_relationship()).then_some(choice.options.len());
                match choice.bounds.min {
                    Some(min) if offered.is_some_and(|offered| min > offered) => {
                        vec![FieldRuleFault::FewerOptions(choice.options.len())]
                    }
                    _ => choice.bounds.empty_range(None).into_iter().collect(),
                }
            }
            ControlKind::Repeater(repeater) => match repeater.bounds.max {
                Some(max) if max > MAX_REPEATER_ROWS => vec![FieldRuleFault::AboveRowCap(max)],
                _ => repeater
                    .bounds
                    .empty_range(Some(MAX_REPEATER_ROWS))
                    .into_iter()
                    .collect(),
            },
            _ => Vec::new(),
        };
        if self.accepted && !self.checkbox {
            faults.push(FieldRuleFault::NotCheckbox);
        }
        let utc = TimeZone::UTC;
        match self.default_value() {
            Some(value) => {
                let spelled = match value {
                    FormValue::Text(text) => text.trim().to_string(),
                    FormValue::List(items) => format!("{items:?}"),
                    FormValue::Rows(_) => "rows".to_string(),
                };
                let unparsed = match &self.control {
                    ControlKind::Text(text) => !spelled.is_empty() && !text.parses(&spelled),
                    _ => false,
                };
                if unparsed {
                    faults.push(FieldRuleFault::UnparsedDefault(spelled));
                } else if self.check(value, &utc).is_some() {
                    faults.push(FieldRuleFault::RefusedDefault(spelled));
                }
            }
            None if self.is_always_disabled() && self.check_unposted(&utc).is_some() => {
                faults.push(FieldRuleFault::RefusedBlank);
            }
            None => {}
        }
        faults
            .into_iter()
            .map(|fault| DeclarationErrorKind::FieldRule {
                field: field.to_string(),
                fault,
            })
            .collect()
    }
}
