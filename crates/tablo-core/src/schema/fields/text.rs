//! The text control: a one-line `<input>`, or a `<textarea>` with `rows`.

use tablo_ui::{input as ui_input, textarea as ui_textarea};
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::{tree::Mode, validation::format_timestamp_input},
    Field, FieldChrome, ValueKind, render_field, render_value,
};
use crate::form::FormScalar;

/// The equality expression a text field's unique probe binds.
type EqProbe = std::sync::Arc<dyn Fn(&str) -> Option<toasty::stmt::Expr<bool>> + Send + Sync>;

/// The text control's input type, placeholder, rows, email rule, and unique probe.
pub(crate) struct TextControl {
    input_type: &'static str,
    pub(super) rows: Option<u32>,
    pub(super) placeholder: Option<String>,
    pub(super) email: bool,
    pub(super) unique: bool,
    probe: Option<EqProbe>,
    /// The stored spelling of a submission, or `None` when the type refuses it.
    spell: fn(&str) -> Option<String>,
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
            rows: None,
            placeholder: None,
            email: false,
            unique,
            probe: Some(probe),
            spell: spell::<T>,
        }
    }

    /// The control of a derived embedded leaf, with no unique probe.
    pub(super) fn leaf<T: FormScalar>() -> Self {
        Self {
            input_type: T::INPUT_TYPE,
            rows: None,
            placeholder: None,
            email: false,
            unique: false,
            probe: None,
            spell: spell::<T>,
        }
    }

    /// Whether two submissions spell the same stored value: `01` and `1` do for an integer.
    pub(crate) fn same_value(&self, a: &str, b: &str) -> bool {
        matches!(((self.spell)(a), (self.spell)(b)), (Some(a), Some(b)) if a == b)
    }

    /// The equality expression the app-side unique check probes with.
    pub(crate) fn eq_filter(&self, value: &str) -> Option<toasty::stmt::Expr<bool>> {
        self.probe.as_ref().and_then(|probe| probe(value))
    }
}

impl Field {
    /// Renders a text field's control and its read-only value.
    pub(super) fn render_text<'a>(
        &self,
        text: &TextControl,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
        mode: Mode,
    ) -> Result<BoxView<'a>> {
        if mode == Mode::View {
            return render_value(cx, self.label_str(), value, ValueKind::Prose);
        }
        let name = self.name().to_string();
        let required = self.is_required();
        let placeholder = text.placeholder.clone();
        let chrome = FieldChrome::new(self.name(), error, None);
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome.described_by();
        let control = if let Some(rows) = text.rows {
            // A `<textarea>` takes its initial value from content, not a `value` attribute.
            let value_owned = value.unwrap_or("").to_string();
            view! {
                cx =>
                ui_textarea(
                    attrs: attributes! {
                        id=(name.clone())
                        name=(name.clone())
                        placeholder=(placeholder.clone())
                        rows=(rows)
                        required=(required)
                        aria-required=(required.then_some("true"))
                        aria-invalid=(aria_invalid)
                        aria-describedby=(described_by)
                    },
                    (value_owned)
                )
            }
            .boxed()
        } else {
            let input_type = if text.email { "email" } else { text.input_type };
            let value_owned = value.map(|s| {
                if text.input_type == "datetime-local" {
                    format_timestamp_input(s)
                } else {
                    s.to_string()
                }
            });
            view! {
                cx =>
                ui_input(
                    attrs: attributes! {
                        id=(name.clone())
                        type=(input_type)
                        name=(name.clone())
                        value=(value_owned.clone())
                        placeholder=(placeholder.clone())
                        required=(required)
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

#[cfg(test)]
mod tests;
