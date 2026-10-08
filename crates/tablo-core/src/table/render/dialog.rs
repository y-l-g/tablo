//! The table's write form, the dialog that confirms its destructive writes, and the dialogs
//! asking for its custom actions' input.

use tablo_ui::{
    ButtonSize, ButtonVariant, alert_dialog, button, dialog_content, dialog_description,
    dialog_footer, dialog_header, dialog_title,
};
use topcoat::{context::Cx, runtime::Event, view::*};

use super::{Frame, table_dom_id};
use crate::table::{
    InputDialog, Posts, action_options_url, open_dialog, selected_count,
    state::{ConfirmSignals, TableSignals, TableState},
};

impl Frame<'_> {
    /// Whether the table posts any write: a delete or a custom action.
    pub(super) fn writes(&self) -> bool {
        self.delete_prefix.is_some() || self.actions_prefix.is_some()
    }

    /// Render the table's one write form inside the dialog that confirms it.
    ///
    /// Every write control names this form: a direct write submits it to its own `formaction`,
    /// and a destructive one opens the dialog as a modal on its POST target, whose submit carries
    /// the `confirm=1` marker the handlers require. Cancel and Escape close the dialog and clear
    /// the target. The form carries the bulk selection, which the row routes ignore.
    pub(super) fn render_write_form<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        signals: &TableSignals,
    ) -> Option<BoxView<'a>> {
        if !self.writes() {
            return None;
        }
        let id = table_dom_id(state, "confirm");
        let form_id = table_dom_id(state, "writes");
        let title_id = format!("{id}-title");
        let description_id = format!("{id}-description");
        let csrf = crate::csrf::current_token(cx);
        let ConfirmSignals {
            action,
            title,
            label,
            bulk: counts,
        } = signals.confirm.clone();
        let selection = signals.bulk.clone();
        let count = selected_count(cx, selection.clone());
        Some(
            view! {
                cx =>
                alert_dialog(
                    open: false,
                    attrs: attributes! {
                        id=(id)
                        aria-labelledby=(title_id.clone())
                        aria-describedby=(description_id.clone())
                        @close=$(|_e: Event| action.set("".to_owned()))
                    },
                    <form
                        id=(form_id)
                        class="contents"
                        method="post"
                        :action=$(action.get())
                    >
                        (crate::csrf::field(cx, &csrf))
                        <input type="hidden" name="confirm" value="1">
                        <input type="hidden" name="ids" :value=$(selection.get())>
                        dialog_content(
                            dialog_header(
                                dialog_title(
                                    attrs: attributes! { id=(title_id) },
                                    $(title.get())
                                )
                                dialog_description(
                                    attrs: attributes! { id=(description_id) },
                                    "This action cannot be undone."
                                    <span :hidden=$(!counts.get())>
                                        " Selected records: "
                                        (count)
                                    </span>
                                )
                            )
                            dialog_footer(
                                button(
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Md,
                                    attrs: attributes! { type="submit" formmethod="dialog" },
                                    "Cancel"
                                )
                                button(
                                    variant: ButtonVariant::Destructive,
                                    size: ButtonSize::Md,
                                    attrs: attributes! { type="submit" },
                                    $(label.get())
                                )
                            )
                        )
                    </form>
                )
            }
            .boxed(),
        )
    }

    /// Render the dialog of each custom action asking for input, which its row and bulk buttons
    /// point at their own POST targets before opening it.
    pub(super) fn render_input_dialogs<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        signals: &TableSignals,
    ) -> Vec<BoxView<'a>> {
        let Some(list) = self.actions_prefix else {
            return Vec::new();
        };
        let form = table_dom_id(state, "writes");
        self.inputs
            .iter()
            .map(|action| {
                let dialog = InputDialog {
                    id: input_dialog_id(&form, action.name),
                    title: action.label.to_string(),
                    label: action.label.to_string(),
                    confirm: action.confirm,
                    input: action.input,
                    options: action_options_url(list, action.name),
                };
                dialog.render(
                    cx,
                    Posts::Table {
                        target: signals.inputs[action.name].clone(),
                        selection: signals.bulk.clone(),
                    },
                )
            })
            .collect()
    }
}

/// The DOM id of the input dialog of the action `name`, in the table whose write form is `form`.
fn input_dialog_id(form: &str, name: &str) -> String {
    format!("{form}-input-{name}")
}

/// The attributes of the button of the action `name`, which asks for input: it opens the
/// action's dialog on `action`, the row's or the bulk route, on the selection when `bulk`.
///
/// Without scripts it submits the table's write `form` to `action`, which renders the input page.
pub(super) fn input_trigger(
    cx: &Cx,
    form: &str,
    signals: &TableSignals,
    name: &'static str,
    action: String,
    bulk: bool,
) -> Attributes {
    let mut attrs = attributes! {
        cx =>
        type="submit"
        form=(form.to_string())
        formaction=(action.clone())
    };
    let target = signals.inputs[name].clone();
    attrs.extend(open_dialog(
        cx,
        &input_dialog_id(form, name),
        Some((target, action, bulk)),
    ));
    attrs
}

/// A write control's attributes: a direct write submits the table's write `form` to `action`, and a
/// destructive one opens the confirmation dialog around the form on it, titled `title` and
/// confirmed by `label`. Both carry `action` as `formaction`; on the dialog's trigger, which
/// submits nothing itself, it only names the write.
/// `bulk` says whether the write takes the selection, which the dialog then counts.
pub(super) fn write_trigger(
    cx: &Cx,
    form: &str,
    signals: &TableSignals,
    action: String,
    confirm: Option<(&'static str, &'static str)>,
    bulk: bool,
) -> Attributes {
    let Some((title, label)) = confirm else {
        return attributes! {
            cx =>
            type="submit"
            form=(form.to_string())
            formaction=(action)
        };
    };
    let ConfirmSignals {
        action: target,
        title: shown_title,
        label: shown_label,
        bulk: counts,
    } = signals.confirm.clone();
    let (title, label, form) = (title.to_owned(), label.to_owned(), form.to_owned());
    attributes! {
        cx =>
        type="button"
        formaction=(action.clone())
        @click=$(|_e: Event| {
            shown_title.set(title.clone());
            shown_label.set(label.clone());
            counts.set(bulk);
            target.set(action.clone());
            raw!(
                "document.getElementById(String(${form})).closest('dialog').showModal()",
                (),
            );
        })
    }
}
