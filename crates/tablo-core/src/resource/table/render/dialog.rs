//! The row-delete confirmation dialog.

use tablo_ui::{
    ButtonSize, ButtonVariant, alert_dialog, button, dialog_content, dialog_description,
    dialog_footer, dialog_header, dialog_title,
};
use topcoat::{Result, context::Cx, view::*};

use super::super::{
    super::state::{TableState, delete_action_url},
    Table,
};

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
                (confirm_controls(cx))
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

/// Submit Cancel, the `confirm=1` marker, and the destructive submit for every delete confirmation.
pub(super) fn confirm_controls<'a>(cx: &'a Cx) -> BoxView<'a> {
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
            "Delete"
        )
    }
    .boxed()
}
