//! The text control: a one-line `<input>`, or a `<textarea>` with `rows`.

use tablo_ui::{input as ui_input, textarea as ui_textarea};
use topcoat::{Result, context::Cx, view::*};

pub(crate) use self::rules::Pattern;
use self::rules::{Order, Rules, order};
use super::{Field, render_field};
use crate::{FieldRuleFault, form::FormScalar};

/// The equality expression a text field's unique probe binds.
type EqProbe = std::sync::Arc<dyn Fn(&str) -> Option<toasty::stmt::Expr<bool>> + Send + Sync>;

/// The text control's input type, placeholder, rows, email and password rules, declared rules,
/// and unique probe.
pub(crate) struct TextControl {
    input_type: &'static str,
    step: Option<&'static str>,
    /// The control's `value` for a stored spelling.
    input_value: fn(&str) -> String,
    pub(super) rows: Option<u32>,
    pub(super) placeholder: Option<String>,
    pub(super) email: bool,
    pub(super) password: bool,
    /// The password the user signs in with today, which a password manager fills, rather than
    /// a new one it would offer to generate.
    pub(super) current_password: bool,
    pub(super) unique: bool,
    pub(super) rules: Rules,
    probe: Option<EqProbe>,
    /// The stored spelling of a submission, or `None` when the type refuses it.
    spell: fn(&str) -> Option<String>,
    /// How the field's type orders two submissions, for its bounds.
    order: Order,
}

/// The stored spelling of a `T` submission.
fn spell<T: FormScalar>(value: &str) -> Option<String> {
    T::parse_form(value.trim())
        .ok()
        .map(|parsed| parsed.to_form())
}

impl TextControl {
    /// Parses a submission into `T` and compares through the lens, so `01` and `1` check as the
    /// stored integer.
    pub(super) fn new<M, T>(path: toasty::stmt::Path<M, T>, unique: bool) -> Self
    where
        T: FormScalar + toasty::stmt::IntoExpr<T> + 'static,
    {
        let path = crate::toasty_compat::model::ModelPath::of(&path);
        let probe: EqProbe = std::sync::Arc::new(move |value: &str| {
            let parsed = T::parse_form(value.trim()).ok()?;
            Some(path.eq(parsed))
        });
        Self {
            input_type: T::INPUT_TYPE,
            step: T::STEP,
            input_value: T::input_value,
            rows: None,
            placeholder: None,
            email: false,
            password: false,
            current_password: false,
            unique,
            rules: Rules::default(),
            probe: Some(probe),
            spell: spell::<T>,
            order: order::<T>,
        }
    }

    /// The control of a derived embedded leaf, with no unique probe.
    pub(super) fn leaf<T: FormScalar>() -> Self {
        Self {
            input_type: T::INPUT_TYPE,
            step: T::STEP,
            input_value: T::input_value,
            rows: None,
            placeholder: None,
            email: false,
            password: false,
            current_password: false,
            unique: false,
            rules: Rules::default(),
            probe: None,
            spell: spell::<T>,
            order: order::<T>,
        }
    }

    /// The control's `value` for the stored spelling `stored`.
    pub(crate) fn rendering(&self, stored: &str) -> String {
        (self.input_value)(stored)
    }

    /// Whether the field's type parses `value`.
    pub(crate) fn parses(&self, value: &str) -> bool {
        (self.spell)(value).is_some()
    }

    /// Whether two submissions spell the same stored value: `01` and `1` do for an integer.
    pub(crate) fn same_value(&self, a: &str, b: &str) -> bool {
        matches!(((self.spell)(a), (self.spell)(b)), (Some(a), Some(b)) if a == b)
    }

    /// The sentence refusing `value`, a trimmed, non-empty submission, for the first declared
    /// rule it breaks. `None` for a value the field's type does not parse: its parse refuses it,
    /// in the type's own words.
    pub(crate) fn refusal(&self, label: &str, value: &str) -> Option<String> {
        if !self.parses(value) {
            return None;
        }
        self.rules.refusal(label, value, self.order)
    }

    /// What is wrong with the declared rules.
    pub(crate) fn rule_faults(&self) -> Vec<FieldRuleFault> {
        self.rules.faults(self.order)
    }

    /// The equality expression the app-side unique check probes with.
    pub(crate) fn eq_filter(&self, value: &str) -> Option<toasty::stmt::Expr<bool>> {
        self.probe.as_ref().and_then(|probe| probe(value))
    }
}

impl Field {
    /// Renders a text field's control.
    pub(super) fn render_text<'a>(
        &self,
        text: &TextControl,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&crate::form::FieldError>,
        id: String,
        disabled: bool,
    ) -> Result<BoxView<'a>> {
        let name = self.name().to_string();
        let required = self.is_required() && !disabled;
        let placeholder = text.placeholder.clone();
        let chrome = self.chrome(id.clone(), error, None);
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome.described_by();
        let rules = &text.rules;
        let (min_length, max_length) = (rules.min_length, rules.max_length);
        let control = if let Some(rows) = text.rows {
            // A `<textarea>` takes its initial value from content, not a `value` attribute.
            let value_owned = value.unwrap_or("").to_string();
            view! {
                cx =>
                ui_textarea(
                    attrs: attributes! {
                        id=(id.clone())
                        name=(name.clone())
                        placeholder=(placeholder.clone())
                        rows=(rows)
                        minlength=(min_length)
                        maxlength=(max_length)
                        required=(required)
                        disabled=(disabled)
                        aria-required=(required.then_some("true"))
                        aria-invalid=(aria_invalid)
                        aria-describedby=(described_by)
                    },
                    (value_owned)
                )
            }
            .boxed()
        } else {
            let input_type = if text.password {
                "password"
            } else if text.email {
                "email"
            } else {
                text.input_type
            };
            // A password never travels back to the browser: a refused form renders it empty.
            let value = value.filter(|_| !text.password);
            let autocomplete = text.password.then_some(if text.current_password {
                "current-password"
            } else {
                "new-password"
            });
            let value_owned = value.map(text.input_value);
            let step = text.step.filter(|_| input_type == text.input_type);
            let min = rules.min.as_deref().map(text.input_value);
            let max = rules.max.as_deref().map(text.input_value);
            let pattern = rules.pattern.as_ref().map(|pattern| pattern.source.clone());
            view! {
                cx =>
                ui_input(
                    attrs: attributes! {
                        id=(id.clone())
                        type=(input_type)
                        step=(step)
                        name=(name.clone())
                        value=(value_owned.clone())
                        placeholder=(placeholder.clone())
                        minlength=(min_length)
                        maxlength=(max_length)
                        min=(min)
                        max=(max)
                        pattern=(pattern)
                        autocomplete=(autocomplete)
                        required=(required)
                        disabled=(disabled)
                        aria-required=(required.then_some("true"))
                        aria-invalid=(aria_invalid)
                        aria-describedby=(described_by)
                    }
                )
            }
            .boxed()
        };
        render_field(
            cx,
            &chrome,
            self.label_str(),
            required,
            attributes! {},
            control,
        )
    }
}

mod rules;
#[cfg(test)]
mod tests;
