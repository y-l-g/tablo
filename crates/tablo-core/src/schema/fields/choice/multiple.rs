//! A choice taking several options: a checkbox per option, each posting the choice's key.

use tablo_ui::{
    FieldLegendVariant, checkbox as ui_checkbox, field_error as ui_field_error, field_legend,
    field_set,
};
use topcoat::{Result, context::Cx, view::*};

use super::{ChoiceControl, OptionLoadError};
use crate::{form::decode_list, schema::fields::Field};

impl Field {
    /// Renders a multiple choice with the DOM id prefix `id`: a checkbox per option, checked when
    /// `value`'s list holds it.
    ///
    /// A hidden empty value comes first, so a submission with no box checked still posts the key
    /// and clears the choice; the fold drops it. A failed or overflowed load keeps the chosen keys
    /// as boxes, so saving the form keeps them.
    pub(super) async fn render_choices<'a>(
        &self,
        choice: &ChoiceControl,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
        id: String,
    ) -> Result<BoxView<'a>> {
        let name = self.name().to_string();
        let label = self.label_str().to_string();
        let chosen = value.and_then(decode_list).unwrap_or_default();
        let loaded = choice.load_options(cx, None).await;
        let denied = matches!(loaded, Err(OptionLoadError::Denied));
        let failed = matches!(
            loaded,
            Err(OptionLoadError::LoadFailed
                | OptionLoadError::Overflow
                | OptionLoadError::Misdeclared)
        );
        let mut options = loaded.unwrap_or_default();
        if failed {
            for key in &chosen {
                if !options.iter().any(|(value, _)| value == key) {
                    options.push((key.clone(), key.clone()));
                }
            }
        }
        let error = match error {
            Some(message) => Some(message.to_string()),
            None => denied.then(|| format!("{label} is not available")),
        };
        let legend_id = format!("{id}-legend");
        let error_id = format!("{id}-error");
        let described_by = error.as_ref().map(|_| error_id.clone());
        let boxes: Vec<BoxView<'a>> = options
            .into_iter()
            .enumerate()
            .map(|(index, (value, text))| {
                let checked = chosen.contains(&value);
                let (id, name) = (format!("{id}-{index}"), name.clone());
                view! {
                    cx =>
                    <label
                        class="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-foreground/5"
                        data-choice=""
                    >
                        ui_checkbox(
                            attrs: attributes! { id=(id) name=(name) value=(value) checked=(checked) }
                        )
                        <span>(text)</span>
                    </label>
                }
                .boxed()
            })
            .collect();
        let empty = boxes.is_empty();
        let searchable = choice.searchable;
        let filter_label = format!("Filter {label} options");
        Ok(view! {
            cx =>
            field_set(
                attrs: attributes! {
                    class=(if error.is_some() {
                        "ac-field ac-field--error gap-2"
                    } else {
                        "ac-field gap-2"
                    })
                    data-invalid=(error.is_some().then_some("true"))
                    data-choices=""
                },
                field_legend(
                    variant: FieldLegendVariant::Label,
                    attrs: attributes! { id=(legend_id.clone()) class="mb-0" },
                    (label)
                )
                <input type="hidden" name=(name) value="">
                if searchable {
                    <input
                        type="search"
                        class="h-9 w-full rounded-lg border border-border bg-transparent px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        placeholder="Filter…"
                        aria-label=(filter_label)
                        autocomplete="off"
                        data-choices-filter=""
                    >
                }
                <div
                    role="group"
                    aria-labelledby=(legend_id)
                    aria-describedby=(described_by)
                    class="flex max-h-60 flex-col overflow-y-auto rounded-lg border border-border p-1"
                >
                    for checkbox in boxes {
                        (checkbox)
                    }
                    if empty {
                        <p class="px-2 py-1.5 text-sm text-muted-foreground">
                            "No options"
                        </p>
                    }
                </div>
                if let Some(message) = error {
                    ui_field_error(
                        attrs: attributes! { id=(error_id) class="ac-error" aria-live="polite" },
                        (message)
                    )
                }
            )
        }
        .boxed())
    }
}
