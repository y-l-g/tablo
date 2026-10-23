//! The chrome every form control renders around itself: its label, help text and error slot.

use tablo_ui::{
    field as ui_field, field_description as ui_field_description, field_error as ui_field_error,
    field_label as ui_field_label,
};
use topcoat::{Result, context::Cx, view::*};

use crate::{
    Text, dom,
    values::{FieldError, FieldErrorKind},
};

/// The validation state a form control renders.
pub(crate) struct FieldChrome {
    /// The control's DOM id, which the label names.
    pub(crate) id: String,
    error_id: String,
    error_text: String,
    /// Why the control renders an error, `None` when it renders none.
    error_kind: Option<&'static str>,
    /// The help text under the control.
    pub(super) help: Option<Text>,
}

/// The `data-error` an error slot carries, naming why the control was refused: a test reads it
/// instead of the sentence.
pub(crate) fn error_kind(error: &FieldError) -> &'static str {
    match error.kind {
        FieldErrorKind::Required => dom::error::REQUIRED,
        FieldErrorKind::Invalid(_) => dom::error::INVALID,
    }
}

impl FieldChrome {
    /// The chrome of the control labelled `label`, refused with `error`. `fallback` is the
    /// message of a choice whose options the user may not read: its slot reads unavailable
    /// whatever refused the submission, and shows `fallback` when nothing did.
    pub(crate) fn new(
        cx: &Cx,
        id: String,
        error: Option<&FieldError>,
        label: &Text,
        fallback: Option<String>,
    ) -> Self {
        let message = error.map(|error| error.message(label).localize(cx));
        let (error_text, error_kind) = match (message, fallback) {
            (Some(message), Some(_)) if !message.is_empty() => {
                (message, Some(dom::error::UNAVAILABLE))
            }
            (Some(message), None) => (message, error.map(error_kind)),
            (_, Some(fallback)) => (fallback, Some(dom::error::UNAVAILABLE)),
            (None, None) => (String::new(), None),
        };
        Self {
            error_id: format!("{id}-error"),
            id,
            error_text,
            error_kind,
            help: None,
        }
    }

    fn has_error(&self) -> bool {
        self.error_kind.is_some()
    }

    fn help_id(&self) -> String {
        format!("{}-description", self.id)
    }

    pub(crate) fn aria_invalid(&self) -> &'static str {
        if self.has_error() { "true" } else { "false" }
    }

    /// The ids of the help text and the error, whichever render.
    pub(crate) fn described_by(&self) -> Option<String> {
        let ids: Vec<String> = [
            self.help.is_some().then(|| self.help_id()),
            self.has_error().then(|| self.error_id.clone()),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!ids.is_empty()).then(|| ids.join(" "))
    }
}

/// The chrome every form control renders: wrapper, label, control, and error slot.
pub(crate) fn render_field<'a>(
    cx: &'a Cx,
    chrome: &FieldChrome,
    label: &Text,
    required: bool,
    attributes: Attributes,
    control: BoxView<'a>,
) -> Result<BoxView<'a>> {
    let id = chrome.id.clone();
    let label_text = label.clone();
    let has_error = chrome.has_error();
    let error_kind = chrome.error_kind;
    let error_id = chrome.error_id.clone();
    let error_text = chrome.error_text.clone();
    let help = chrome.help.clone();
    let help_id = chrome.help_id();
    let field_class = if has_error {
        "ac-field ac-field--error"
    } else {
        "ac-field"
    };
    Ok(view! {
        cx =>
        ui_field(
            attrs: attributes! {
                class=(field_class)
                data-invalid=(has_error.then_some("true"))
                (attributes)
            },
            ui_field_label(
                attrs: attributes! { for=(id) },
                (label_text)
                if required {
                    <span class="text-destructive" aria-hidden="true">"*"</span>
                }
            )
            (control)
            if let Some(help) = help {
                ui_field_description(attrs: attributes! { id=(help_id) }, (help))
            }
            if has_error {
                ui_field_error(
                    attrs: attributes! {
                        id=(error_id)
                        class="ac-error"
                        aria-live="polite"
                        data-error=(error_kind)
                    },
                    (error_text)
                )
            }
        )
    }
    .boxed())
}
