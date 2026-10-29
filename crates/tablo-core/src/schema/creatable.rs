//! What mounting refuses of a creatable choice.

use super::{ChoiceControl, Field, Schema};

impl<F> Schema<F> {
    /// One [`UncreatableChoice`](crate::DeclarationErrorKind::UncreatableChoice) per creatable
    /// choice that is not a single, independent select over a resource's records, per one a
    /// condition can disable, whose link would set a value the form does not post, and per one in
    /// a repeater's rows, whose rows the browser copies from a template.
    pub(super) fn creatable_errors(&self) -> Vec<crate::DeclarationErrorKind> {
        let creatable = |field: &Field| field.as_choice().is_some_and(ChoiceControl::is_creatable);
        let mut errors = Vec::new();
        for field in &self.fields {
            let disabled =
                |choice: &ChoiceControl| choice.is_creatable() && field.disabling().is_some();
            if field
                .as_choice()
                .is_some_and(|choice| choice.uncreatable() || disabled(choice))
            {
                errors.push(crate::DeclarationErrorKind::UncreatableChoice {
                    field: field.name().to_string(),
                });
            }
            if let Some(repeater) = field.as_repeater() {
                errors.extend(
                    repeater
                        .item_schema()
                        .fields()
                        .filter(|leaf| creatable(leaf))
                        .map(|leaf| crate::DeclarationErrorKind::UncreatableChoice {
                            field: format!("{}.{}", field.name(), leaf.name()),
                        }),
                );
            }
        }
        errors
    }
}
