//! The row-delete confirmation dialog.

use tablo_ui::{
    ButtonSize, ButtonVariant, alert_dialog, button, dialog_content, dialog_description,
    dialog_footer, dialog_header, dialog_title,
};
use topcoat::{Result, context::Cx, view::*};

use super::super::{super::state::delete_action_url, NormalizedState, Table};

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
    pub(crate) async fn render_delete_dialog_normalized<'a>(
        &self,
        cx: &'a Cx,
        state: &NormalizedState,
    ) -> Result<Option<BoxView<'a>>> {
        let Some(prefix) = self.delete_prefix.as_deref() else {
            return Ok(None);
        };
        let key = state
            .delete
            .as_deref()
            .filter(|_| state.open != Some(false));
        let server_open = key.is_some();
        let action = key.map(|key| delete_action_url(prefix, key));
        // Only the URL-driven dialog mirrors its dismissal into the URL: a
        // dialog a row control opens client-side has no `?delete=` to close,
        // so dismissing it leaves the URL alone (GH #154 §3).
        let open_param = server_open.then_some("open");
        let dialog_id = Self::delete_dialog_dom_id(prefix);
        let title_id = format!("{dialog_id}-title");
        let description_id = format!("{dialog_id}-description");
        let csrf = crate::csrf::current_token(cx);
        Ok(Some(
            view! {
                cx =>
                alert_dialog(
                    open: server_open,
                    attrs: attributes! {
                        id=(dialog_id)
                        aria-labelledby=(title_id.clone())
                        aria-describedby=(description_id.clone())
                        data-dialog-open-param=(open_param)
                    },
                    dialog_content(
                        dialog_header(
                            dialog_title(
                                attrs: attributes! { id=(title_id.clone()) },
                                "Delete this record?"
                            )
                            dialog_description(
                                attrs: attributes! { id=(description_id.clone()) },
                                "This action cannot be undone."
                            )
                        )
                        dialog_footer(
                            <form
                                method="post"
                                action=(action)
                                class="contents"
                                data-row-delete-form=""
                                data-mutation-submit=""
                            >
                                button(
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Md,
                                    attrs: attributes! { type="button" data-dialog-close="" },
                                    "Cancel"
                                )
                                <input type="hidden" name="confirm" value="1">
                                (crate::csrf::field(cx, &csrf))
                                button(
                                    variant: ButtonVariant::Destructive,
                                    size: ButtonSize::Md,
                                    attrs: attributes! { type="submit" },
                                    "Delete"
                                )
                            </form>
                        )
                    )
                )
            }
            .boxed(),
        ))
    }

    /// The DOM id of a table's row-delete dialog: the delete prefix
    /// with its slashes flattened, so two tables with different delete prefixes
    /// never share an id. Two tables over one prefix (a page rendering the same
    /// resource twice) derive the same ids; the panel renders one list table
    /// per page — the list parameters are shared — so its own routes cannot
    /// reach that. The row controls name the dialog they open, and its
    /// `aria-labelledby`/`aria-describedby` ids derive from it.
    pub(super) fn delete_dialog_dom_id(prefix: &str) -> String {
        format!("{}-delete-dialog", prefix.replace('/', "-"))
    }
}
