//! A choice listing an input per option, each posting the choice's key: a checkbox when it takes
//! several options, else a radio, either drawn as a button under [`Layout::Buttons`].

use tablo_ui::{
    FieldLegendVariant, checkbox as ui_checkbox, field_description as ui_field_description,
    field_error as ui_field_error, field_legend, field_set, radio_group_item,
};
use topcoat::{Result, context::Cx, icon::icon, view::*};

use super::{ChoiceControl, OptionLoadError};
use crate::schema::fields::{Field, Posting};

/// How a choice renders its options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Layout {
    /// A `<select>`, or a checkbox list when the choice takes several.
    #[default]
    Select,
    /// A radio per option, or a checkbox list when the choice takes several.
    Radio,
    /// A button per option, pressed while its option is chosen.
    Buttons,
}

/// The classes of an option drawn as a button: its input is visually hidden, and the label
/// takes the input's checked, focused and disabled states.
const BUTTON: &str = "inline-flex h-9 cursor-pointer select-none items-center gap-1.5 rounded-md \
     border border-border bg-background px-3 text-sm font-medium transition-colors \
     hover:bg-foreground/5 [&>svg]:size-4 [&>svg]:shrink-0 has-[:focus-visible]:ring-2 \
     has-[:focus-visible]:ring-ring has-[:focus-visible]:ring-offset-2 \
     has-[:focus-visible]:ring-offset-background has-[:disabled]:pointer-events-none \
     has-[:disabled]:opacity-50";

/// The classes of a chosen button whose option names no color: filled with the primary color.
const PLAIN_BUTTON: &str = "has-[:checked]:border-primary has-[:checked]:bg-primary \
     has-[:checked]:text-primary-foreground";

/// The classes of an option row in a list.
const ROW: &str = "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-foreground/5";

impl Field {
    /// Renders a listed choice with the DOM id prefix `id`: an input per option, checked when
    /// `chosen` holds it.
    ///
    /// A hidden empty value comes first, so a submission with no option checked still posts the
    /// key and clears the choice: a multiple choice's read drops it, and a single choice's checked
    /// radio, posted after it, replaces it. A failed or overflowed load keeps the chosen keys as
    /// options, so saving the form keeps them.
    pub(super) async fn render_choices<'a>(
        &self,
        choice: &ChoiceControl,
        cx: &'a Cx,
        chosen: &[String],
        error: Option<&crate::values::FieldError>,
        posting: Posting,
    ) -> Result<BoxView<'a>> {
        let Posting { id, name, disabled } = posting;
        let label = self.label();
        let multiple = choice.multiple;
        let buttons = choice.layout == Layout::Buttons;
        // A radio group's `required` makes the browser ask for one; a checkbox's would ask for
        // every box.
        let required = self.required && !disabled;
        let radio_required = required && !multiple;
        let loaded = choice.load_options(cx, None).await;
        let denied = matches!(loaded, Err(OptionLoadError::Denied));
        let failed = matches!(
            loaded,
            Err(OptionLoadError::LoadFailed
                | OptionLoadError::Overflow
                | OptionLoadError::Misdeclared)
        );
        let mut options = loaded.unwrap_or_default();
        // A tags choice offers each value it holds, its suggestions or not.
        if failed || choice.tags {
            for key in chosen {
                if !options.iter().any(|(value, _)| value == key) {
                    options.push((key.clone(), key.clone()));
                }
            }
        }
        // A denied choice reads unavailable whatever refused the submission.
        let (error, error_kind) = match error {
            Some(error) if denied => (
                Some(error.message(&label).localize(cx)),
                Some(crate::dom::error::UNAVAILABLE),
            ),
            Some(error) => (
                Some(error.message(&label).localize(cx)),
                Some(super::super::error_kind(error)),
            ),
            None if denied => (
                Some(crate::t!(
                    cx,
                    "tablo-field-unavailable",
                    label = label.clone()
                )),
                Some(crate::dom::error::UNAVAILABLE),
            ),
            None => (None, None),
        };
        let legend_id = format!("{id}-legend");
        let error_id = format!("{id}-error");
        let help = self.help.clone();
        let help_id = format!("{id}-description");
        let described_by = [
            help.as_ref().map(|_| help_id.clone()),
            error.as_ref().map(|_| error_id.clone()),
        ]
        .into_iter()
        .flatten()
        .reduce(|ids, id| format!("{ids} {id}"));
        let inputs: Vec<BoxView<'a>> = options
            .into_iter()
            .enumerate()
            .map(|(index, (value, text))| {
                let checked = chosen.contains(&value);
                let (id, name) = (format!("{id}-{index}"), name.clone());
                if buttons {
                    let kind = if multiple { "checkbox" } else { "radio" };
                    let (color, glyph) = choice
                        .faces
                        .iter()
                        .find(|(face, ..)| *face == value)
                        .map(|(_, color, glyph)| (*color, glyph.clone()))
                        .unwrap_or_default();
                    let class = format!(
                        "{BUTTON} {}",
                        color.map_or(PLAIN_BUTTON, crate::Color::toggle_class)
                    );
                    return view! {
                        <label class=(class) data-choice="">
                            if let Some(data) = glyph {
                                icon(data: data, attrs: attributes! { aria-hidden="true" })
                            }
                            <input
                                type=(kind)
                                class="sr-only"
                                id=(id)
                                name=(name)
                                value=(value)
                                checked=(checked)
                                required=(radio_required)
                            >
                            <span>(text)</span>
                        </label>
                    }
                    .boxed();
                }
                let attrs = attributes! {
                    id=(id)
                    name=(name)
                    value=(value)
                    checked=(checked)
                    required=(radio_required)
                };
                view! {
                    <label class=(ROW) data-choice="">
                        if multiple {
                            ui_checkbox(attrs: attrs)
                        } else {
                            radio_group_item(attrs: attrs)
                        }
                        <span>(text)</span>
                    </label>
                }
                .boxed()
            })
            .collect();
        let empty = inputs.is_empty() && !choice.tags;
        let tags = choice.tags.then(|| {
            (
                format!("{id}-new"),
                name.clone(),
                crate::t!(cx, "tablo-field-add-tags", label = label.clone()),
                described_by.clone(),
            )
        });
        let searchable = choice.searchable;
        let filter_label = crate::t!(cx, "tablo-field-filter-options", label = label.clone());
        let role = if multiple { "group" } else { "radiogroup" };
        // A checkbox list may run long, so it scrolls in a box; a radio list is a few options.
        let list_class = if buttons {
            "flex flex-wrap gap-2"
        } else if multiple {
            "flex max-h-60 flex-col overflow-y-auto rounded-lg border border-border p-1"
        } else {
            "flex flex-col"
        };
        Ok(view! {
            field_set(
                attrs: attributes! {
                    class=(if error.is_some() {
                        "ac-field ac-field--error gap-2"
                    } else {
                        "ac-field gap-2"
                    })
                    data-invalid=(error.is_some().then_some("true"))
                    data-choices=""
                    disabled=(disabled)
                },
                field_legend(
                    variant: FieldLegendVariant::Label,
                    attrs: attributes! { id=(legend_id.clone()) class="mb-0" },
                    (label)
                    if required {
                        <span class="ms-0.5 text-destructive" aria-hidden="true">
                            "*"
                        </span>
                    }
                )
                <input type="hidden" name=(name) value="">
                if searchable {
                    <input
                        type="search"
                        class="h-9 w-full rounded-lg border border-border bg-transparent px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        placeholder=(crate::t!(cx, "tablo-field-filter"))
                        aria-label=(filter_label)
                        autocomplete="off"
                        data-choices-filter=""
                    >
                }
                <div
                    role=(role)
                    aria-labelledby=(legend_id)
                    aria-describedby=(described_by)
                    aria-required=(radio_required.then_some("true"))
                    class=(list_class)
                >
                    for input in inputs {
                        (input)
                    }
                    if empty {
                        <p class="px-2 py-1.5 text-sm text-muted-foreground">
                            (crate::t!(cx, "tablo-field-no-options"))
                        </p>
                    }
                    // The filter shows it when it hides every option.
                    if searchable && !empty {
                        <p
                            class="px-2 py-1.5 text-sm text-muted-foreground"
                            data-choices-unmatched=""
                            hidden=""
                        >
                            (crate::t!(cx, "tablo-field-no-matching-options"))
                        </p>
                    }
                </div>
                if let Some((new_id, new_name, placeholder, new_described_by)) = tags {
                    <input
                        type="text"
                        id=(new_id)
                        name=(new_name)
                        class="h-9 w-full rounded-lg border border-border bg-transparent px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        placeholder=(placeholder.clone())
                        aria-label=(placeholder)
                        aria-describedby=(new_described_by)
                        autocomplete="off"
                        data-choices-add=""
                    >
                }
                if let Some(help) = help {
                    ui_field_description(attrs: attributes! { id=(help_id) }, (help))
                }
                if let Some(message) = error {
                    ui_field_error(
                        attrs: attributes! {
                            id=(error_id)
                            class="ac-error"
                            aria-live="polite"
                            data-error=(error_kind)
                        },
                        (message)
                    )
                }
            )
        }
        .boxed())
    }
}
