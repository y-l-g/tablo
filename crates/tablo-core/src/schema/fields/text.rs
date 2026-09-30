//! The text control: a one-line `<input>`, or a `<textarea>` with `rows`.

use tablo_ui::{input as ui_input, textarea as ui_textarea};
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::{lenses::FieldLens, tree::Mode, validation::format_timestamp_input},
    Field, FieldChrome, ValueKind, render_field, render_value,
};
use crate::form::FormScalar;

/// The equality expression a text field's unique probe binds.
///
/// `None` means the submitted value does not parse into the field's type:
/// validation has already refused it, and the probe has nothing to compare.
type EqProbe = std::sync::Arc<dyn Fn(&str) -> Option<toasty::stmt::Expr<bool>> + Send + Sync>;

/// What a text field declares beyond presence and its rules.
pub(crate) struct TextControl {
    /// The control's `type`: the scalar type's `INPUT_TYPE` (`email`
    /// overrides it at render).
    input_type: &'static str,
    /// `Some` renders a `<textarea>` of that many lines.
    pub(super) rows: Option<u32>,
    pub(super) placeholder: Option<String>,
    pub(super) unique: bool,
    probe: EqProbe,
}

impl TextControl {
    /// The control for a `T` column at `path`, its unique marker defaulting to
    /// `unique`.
    ///
    /// The probe parses a submission into `T` and compares it through `path`,
    /// so a value unique as text but not as the type (`01` and `1` to an
    /// integer column) is checked for what the record will store, and an
    /// embedded leaf compares its own flattened column.
    pub(super) fn new<M, T>(path: FieldLens<M, T>, unique: bool) -> Self
    where
        T: FormScalar + toasty::stmt::IntoExpr<T> + 'static,
    {
        // The untyped path, so the probe holds nothing of `M`: the same
        // expression `Path::eq` builds.
        let path: toasty_core::stmt::Path = path.into();
        let probe: EqProbe = std::sync::Arc::new(move |value: &str| {
            let parsed = T::parse_form(value.trim()).ok()?;
            let rhs: toasty_core::stmt::Expr = toasty::stmt::IntoExpr::into_expr(parsed).into();
            Some(toasty::stmt::Expr::from_untyped(
                toasty_core::stmt::Expr::eq(path.clone().into_stmt(), rhs),
            ))
        });
        Self {
            input_type: T::INPUT_TYPE,
            rows: None,
            placeholder: None,
            unique,
            probe,
        }
    }

    /// The equality expression the app-side unique check probes with, or
    /// `None` when the submission does not parse.
    pub(crate) fn eq_filter(&self, value: &str) -> Option<toasty::stmt::Expr<bool>> {
        (self.probe)(value)
    }
}

impl Field {
    /// Render a text field: a read-only value in `Mode::View`, the control
    /// with `value` and `errors` otherwise.
    pub(super) fn render_text<'a>(
        &self,
        text: &TextControl,
        cx: &'a Cx,
        value: Option<&str>,
        errors: &[String],
        mode: Mode,
    ) -> Result<BoxView<'a>> {
        if mode == Mode::View {
            let kind = if text.rows.is_some() {
                ValueKind::Prose
            } else {
                ValueKind::Machine
            };
            return render_value(cx, &self.label, value, kind);
        }
        let name = self.name.clone();
        // The marker reads the same predicate validation uses, so a unique
        // field is never refused for emptiness while rendering as optional.
        let required = self.is_required();
        let placeholder = text.placeholder.clone();
        let chrome = FieldChrome::new(&self.name, errors, None);
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome.described_by();
        let control = if let Some(rows) = text.rows {
            // A `<textarea>` takes its initial value from content, not a
            // `value` attribute.
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
            let input_type = if self.rules.is_email() {
                "email"
            } else {
                text.input_type
            };
            // A timestamp renders its UTC `datetime-local` spelling, not the
            // stored RFC 3339: the control carries no zone.
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
        render_field(cx, &chrome, &self.label, required, attributes! {}, control)
    }
}

#[cfg(test)]
mod tests;
