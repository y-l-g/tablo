//! The controls of an editable cell: free functions over owned data, so tablo-core compiles each
//! once rather than once for each model and field type. The cell's form submits them.

use tablo_ui::{input as ui_input, select as ui_select, switch as ui_switch};
use topcoat::{context::Cx, view::*};

/// A switch posting `name`, `true` when checked and the hidden `false` otherwise, as the edit
/// form's checkbox does.
pub(super) fn toggle<'a>(cx: &'a Cx, name: &str, label: String, checked: bool) -> BoxView<'a> {
    let name = name.to_string();
    view! {
        cx =>
        <input type="hidden" name=(name.clone()) value="false">
        ui_switch(
            attrs: attributes! {
                cx =>
                name=(name)
                value="true"
                checked=(checked)
                aria-label=(label)
            }
        )
    }
    .boxed()
}

/// A select posting `name`, with `selected` chosen among `options`, each a value and its label.
pub(super) fn select<'a>(
    cx: &'a Cx,
    name: &str,
    label: String,
    selected: String,
    options: Vec<(String, String)>,
) -> BoxView<'a> {
    let name = name.to_string();
    view! {
        cx =>
        ui_select(
            attrs: attributes! { cx => name=(name) aria-label=(label) class="min-w-32" },
            for (value, text) in options {
                <option value=(value.clone()) selected=(value == selected)>
                    (text)
                </option>
            }
        )
    }
    .boxed()
}

/// What a text input cell shows.
pub(super) struct TextInput<'n> {
    pub(super) name: &'n str,
    pub(super) label: String,
    /// The input's value, as the edit form's control shows it.
    pub(super) value: String,
    pub(super) input_type: &'static str,
    pub(super) step: Option<&'static str>,
}

/// A text input posting its value.
pub(super) fn text_input<'a>(cx: &'a Cx, input: TextInput<'_>) -> BoxView<'a> {
    let name = input.name.to_string();
    view! {
        cx =>
        ui_input(
            attrs: attributes! {
                cx =>
                type=(input.input_type)
                name=(name)
                value=(input.value)
                step=(input.step)
                aria-label=(input.label)
                class="h-8 min-w-24"
            }
        )
    }
    .boxed()
}
