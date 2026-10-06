//! The table's write form and the dialog that confirms its destructive writes.

use tablo_ui::{
    ButtonSize, ButtonVariant, alert_dialog, button, dialog_content, dialog_description,
    dialog_footer, dialog_header, dialog_title,
};
use topcoat::{
    context::Cx,
    runtime::{Event, expr},
    view::*,
};

use super::{super::WiredTable, table_dom_id};
use crate::table::state::{ConfirmSignals, TableSignals, TableState};

impl<M> WiredTable<M> {
    /// Whether the table posts any write: a delete or a custom action.
    pub(super) fn writes(&self) -> bool {
        self.delete_prefix().is_some() || self.actions_prefix().is_some()
    }

    /// Render the table's one write form inside the dialog that confirms it.
    ///
    /// Every write control names this form: a direct write submits it to its own `formaction`,
    /// and a destructive one opens the dialog on its POST target, whose submit carries the
    /// `confirm=1` marker the handlers require. The form carries the bulk selection, which the
    /// row routes ignore.
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
        let count = expr!({
            let wire = selection.get();
            raw!(
                "cx.hydrate(String(String(${wire}).split(',').filter(Boolean).length))",
                wire.split(',')
                    .filter(|key| !key.is_empty())
                    .count()
                    .to_string()
            )
        });
        Some(
            view! {
                cx =>
                <form id=(form_id) method="post" :action=$(action.get())>
                    (crate::csrf::field(cx, &csrf))
                    <input type="hidden" name="confirm" value="1">
                    <input type="hidden" name="ids" :value=$(selection.get())>
                    alert_dialog(
                        open: expr!(!action.get().is_empty()),
                        attrs: attributes! {
                            id=(id)
                            aria-labelledby=(title_id.clone())
                            aria-describedby=(description_id.clone())
                            @keydown=$(|e: Event| {
                                if e.key == "Escape" {
                                    action.set("".to_owned());
                                }
                            })
                        },
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
                                    attrs: attributes! {
                                        type="button"
                                        @click=$(|_e: Event| action.set("".to_owned()))
                                    },
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
                    )
                </form>
            }
            .boxed(),
        )
    }
}

/// A write control's attributes: a direct write submits the table's write `form` to `action`, and a
/// destructive one opens the confirmation dialog on it, titled `title` and confirmed by `label`,
/// and focuses its Cancel. Both carry `action` as `formaction`; on the dialog's trigger, which
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
    // The dialog takes focus once it shows, so Escape reaches it and the page behind is left.
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
                "setTimeout(() => document.getElementById(String(${form}))?.querySelector('dialog button')?.focus())",
                (),
            );
        })
    }
}
