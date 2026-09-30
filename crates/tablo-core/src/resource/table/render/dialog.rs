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
    /// The row-delete confirmation dialog, rendered with the table the panel
    /// wired the delete route into.
    ///
    /// One dialog per table: the row Delete controls name it
    /// (`data-row-delete-trigger`) and carry the record's POST target
    /// (`data-row-delete-action`), which `assets/dialog.js` writes to the form
    /// before opening it in place. The control keeps its `?delete=<row key>`
    /// href, so a page without the script opens the dialog through the URL —
    /// and that render ships it open. `?open=false`, the mirror `dialog.js`
    /// writes on dismissal ([`TableState::open`](crate::resource::TableState::open)), leaves it
    /// closed. Cancel is a `data-dialog-close` button on both paths, so dismissal never
    /// navigates.
    ///
    /// [`Self::render_with_state`] renders it with the table; the live-search
    /// page (`panel::resource_list_live`) calls this separately because the
    /// shard swaps the table per keystroke and must not carry dialog state.
    ///
    /// Escape/backdrop dismissal, the `data-dialog-close` cancel hook and the
    /// trigger wiring need `assets/dialog.js` (`tablo_ui::DIALOG_JS`),
    /// emitted by `Panel::render_document` on every document with shell assets
    /// (ADR-0014). Without the document scripts Cancel is inert and Delete still
    /// POSTs; the dialog primitives are vendored under the ADR-0007 sync guard,
    /// so they carry no note themselves.
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
        // Only the URL-driven dialog mirrors its dismissal into the URL: a
        // dialog a row control opens client-side has no `?delete=` to close,
        // so dismissing it leaves the URL alone (GH #154 §3).
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

    /// The DOM id of a table's row-delete dialog: the delete prefix
    /// with its slashes flattened, so two tables with different delete prefixes
    /// never share an id. Two tables over one prefix (a page rendering the same
    /// resource twice) derive the same ids; the panel renders one list table
    /// per page — the list parameters are shared — so its own routes cannot
    /// reach that. The row controls name the dialog they open, and its
    /// `aria-labelledby`/`aria-describedby` ids derive from it.
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
    /// Extra attributes on the dialog element.
    pub(super) attrs: Attributes,
    /// Extra attributes on the description.
    pub(super) description_attrs: Attributes,
    /// The footer: [`confirm_controls`], wrapped in a form when the dialog is
    /// not already inside the one it submits.
    pub(super) footer: BoxView<'a>,
}

/// An alert dialog asking to confirm a destructive action, labelled by its
/// title and described by "This action cannot be undone.".
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

/// The controls every delete confirmation submits: Cancel (closes the dialog
/// through `dialog.js`), the `confirm=1` marker the handler requires, and the
/// destructive submit.
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
