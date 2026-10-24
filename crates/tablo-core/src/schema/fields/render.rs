//! Rendering a [`Field`]: its control where a form places it, or its stored value read only.

use topcoat::{Result, context::Cx, view::*};

use super::{
    Control, ControlInput, ControlKind, Field, Placement, read_only, render_field, value_cell,
};
use crate::values::{FieldError, FormValue};

impl Field {
    /// Renders the field's control where `placement` puts it.
    pub(crate) async fn render_under<'a>(
        &self,
        cx: &'a Cx,
        value: Option<&FormValue>,
        error: Option<&FieldError>,
        placement: Placement<'_>,
    ) -> Result<BoxView<'a>> {
        let id = placement.id(self.name());
        let disabled = self.is_disabled_at(&placement);
        let text = value.and_then(FormValue::as_text);
        match &self.control {
            ControlKind::Text(control) => self.render_text(control, cx, text, error, id, disabled),
            #[cfg(feature = "rich-text")]
            ControlKind::RichText(rich) => {
                self.render_rich_text(rich, cx, text, error, id, disabled)
            }
            ControlKind::Choice(choice) => {
                Box::pin(self.render_choice(choice, cx, value, error, id, placement)).await
            }
            ControlKind::File(file) => self.render_file(file, cx, text, error, id, disabled),
            ControlKind::Custom(control) => {
                self.render_custom(control.as_ref(), cx, text, error, id, disabled)
            }
            // Its node renders it, from the keys and errors of each row's controls.
            ControlKind::Repeater(_) => Ok(().boxed()),
        }
    }

    /// The stored `value` as a reader sees it: a choice's option label, else the value itself.
    /// `None` for a variant control whose value names no variant.
    pub(crate) fn read(&self, cx: &Cx, value: &str) -> Option<String> {
        match &self.control {
            ControlKind::Choice(choice) => {
                let stored = value.trim();
                match choice.label_of(stored) {
                    Some(label) => Some(label.localize(cx)),
                    None if choice.discriminant => None,
                    None => Some(stored.to_string()),
                }
            }
            _ => Some(value.to_string()),
        }
    }

    /// Renders the label over the stored `value`, [read](Self::read) rather than edited: an
    /// embedded value's leaf or variant on a detail page.
    pub(crate) fn display<'a>(&self, cx: &'a Cx, value: &str) -> BoxView<'a> {
        match self.read(cx, value) {
            Some(text) => read_only(cx, &self.label(), value_cell(cx, &text)),
            None => ().boxed(),
        }
    }

    /// Renders an app [`Control`] in its shared chrome.
    fn render_custom<'a>(
        &self,
        control: &dyn Control,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&FieldError>,
        id: String,
        disabled: bool,
    ) -> Result<BoxView<'a>> {
        let required = (self.is_required() || self.accepted) && !disabled;
        let chrome = self.chrome(cx, id, error, None);
        let input = ControlInput::new(
            self.name(),
            &chrome.id,
            value,
            required,
            chrome.aria_invalid() == "true",
            chrome.described_by(),
        )
        .disabled(disabled);
        let rendered = control.render(cx, input);
        render_field(
            cx,
            &chrome,
            &self.label(),
            required,
            attributes! {},
            rendered,
        )
    }
}
