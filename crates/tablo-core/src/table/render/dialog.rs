//! The row-delete confirmation dialog.

use tablo_ui::{
    ButtonSize, ButtonVariant, alert_dialog, button, dialog_content, dialog_description,
    dialog_footer, dialog_header, dialog_title,
};
use topcoat::{Result, context::Cx, view::*};

use super::super::Table;
use crate::table::state::{TableState, delete_action_url};

impl<M> Table<M> {
    /// Render the row-delete confirmation dialog for a table with the delete route wired.
    pub(crate) async fn render_delete_dialog<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
    ) -> Result<Option<BoxView<'a>>> {
        let Some(prefix) = self.delete_prefix.as_deref() else {
            return Ok(None);
        };
        let key = state
            .delete
            .as_deref()
            .filter(|_| state.open != Some(false));
        let server_open = key.is_some();
        let action = key.map(|key| self.action_url(delete_action_url(prefix, key)));
        let open_param = server_open.then(|| state.param("open"));
        let csrf = crate::csrf::current_token(cx);
        let footer = view! {
            cx =>
            <form
                method="post"
                action=(action)
                class="contents"
                data-row-delete-form=""
                data-mutation-submit=""
            >
                (crate::csrf::field(cx, &csrf))
                (confirm_controls(cx, "Delete"))
            </form>
        }
        .boxed();
        Ok(Some(confirm_dialog(
            cx,
            ConfirmDialog {
                id: Self::delete_dialog_dom_id(prefix),
                open: server_open,
                title: "Delete this record?",
                attrs: attributes! { cx => data-dialog-open-param=(open_param) },
                description_attrs: Attributes::default(),
                footer,
            },
        )))
    }

    /// Derive a table's row-delete dialog DOM id from its delete prefix.
    pub(super) fn delete_dialog_dom_id(prefix: &str) -> String {
        chrome_dom_id(prefix, "delete-dialog")
    }

    /// Render the shared confirmatory-action dialog for a table wiring one,
    /// borrowing the row-delete dialog mechanism: the trigger names this
    /// dialog and carries its POST target.
    pub(crate) fn render_action_confirm_dialog<'a>(&self, cx: &'a Cx) -> Option<BoxView<'a>> {
        let prefix = self.actions_prefix.as_deref()?;
        if !self.custom_actions.iter().any(|action| action.confirm) {
            return None;
        }
        let csrf = crate::csrf::current_token(cx);
        let footer = view! {
            cx =>
            <form
                method="post"
                class="contents"
                data-row-delete-form=""
                data-mutation-submit=""
            >
                (crate::csrf::field(cx, &csrf))
                (confirm_controls(cx, "Confirm"))
            </form>
        }
        .boxed();
        Some(confirm_dialog(
            cx,
            ConfirmDialog {
                id: Self::action_confirm_dialog_dom_id(prefix),
                open: false,
                title: "Run this action?",
                attrs: Attributes::default(),
                description_attrs: Attributes::default(),
                footer,
            },
        ))
    }

    /// Derive a table's confirmatory-action dialog DOM id from its actions prefix.
    pub(super) fn action_confirm_dialog_dom_id(prefix: &str) -> String {
        chrome_dom_id(prefix, "action-confirm")
    }
}

/// The DOM id of one piece of a table's chrome: the delete prefix with its
/// slashes flattened, then `suffix`, so two tables with different delete
/// prefixes never share an id.
pub(super) fn chrome_dom_id(prefix: &str, suffix: &str) -> String {
    format!("{}-{suffix}", prefix.replace('/', "-"))
}

/// A destructive confirmation dialog.
pub(super) struct ConfirmDialog<'a> {
    /// The dialog's DOM id; the title and description ids derive from it.
    pub(super) id: String,
    /// Whether it renders open (a URL-driven dialog) or closed.
    pub(super) open: bool,
    pub(super) title: &'static str,
    pub(super) attrs: Attributes,
    pub(super) description_attrs: Attributes,
    /// The footer submitting the dialog's form.
    pub(super) footer: BoxView<'a>,
}

/// Confirm a destructive action in an alert dialog.
pub(super) fn confirm_dialog<'a>(cx: &'a Cx, dialog: ConfirmDialog<'a>) -> BoxView<'a> {
    let ConfirmDialog {
        id,
        open,
        title,
        attrs: extra,
        description_attrs: extra_description,
        footer,
    } = dialog;
    let title_id = format!("{id}-title");
    let description_id = format!("{id}-description");
    let mut attrs = attributes! {
        cx =>
        id=(id)
        aria-labelledby=(title_id.clone())
        aria-describedby=(description_id.clone())
    };
    attrs.extend(extra);
    let mut description_attrs = attributes! { cx => id=(description_id) };
    description_attrs.extend(extra_description);
    view! {
        cx =>
        alert_dialog(
            open: open,
            attrs: attrs,
            dialog_content(
                dialog_header(
                    dialog_title(attrs: attributes! { id=(title_id) }, (title))
                    dialog_description(
                        attrs: description_attrs,
                        "This action cannot be undone."
                    )
                )
                dialog_footer((footer))
            )
        )
    }
    .boxed()
}

/// Submit Cancel, the `confirm=1` marker, and the labeled submit for a confirmation dialog.
pub(super) fn confirm_controls<'a>(cx: &'a Cx, submit: &'static str) -> BoxView<'a> {
    view! {
        cx =>
        button(
            variant: ButtonVariant::Outline,
            size: ButtonSize::Md,
            attrs: attributes! { type="button" data-dialog-close="" },
            "Cancel"
        )
        <input type="hidden" name="confirm" value="1">
        button(
            variant: ButtonVariant::Destructive,
            size: ButtonSize::Md,
            attrs: attributes! { type="submit" },
            (submit)
        )
    }
    .boxed()
}

/// Submit Cancel, the `confirm=1` marker, and the submit the bulk
/// trigger retargets through `formaction`.
pub(super) fn bulk_action_confirm_controls<'a>(cx: &'a Cx) -> BoxView<'a> {
    view! {
        cx =>
        button(
            variant: ButtonVariant::Outline,
            size: ButtonSize::Md,
            attrs: attributes! { type="button" data-dialog-close="" },
            "Cancel"
        )
        <input type="hidden" name="confirm" value="1">
        button(
            variant: ButtonVariant::Destructive,
            size: ButtonSize::Md,
            attrs: attributes! { type="submit" data-bulk-action-confirm-submit="" },
            "Confirm"
        )
    }
    .boxed()
}
