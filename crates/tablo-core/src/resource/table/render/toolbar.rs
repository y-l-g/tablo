//! The search bar, live-search bar, and bulk-action bar.

use tablo_ui::{
    ButtonSize, ButtonVariant, alert_dialog, button, dialog_content, dialog_description,
    dialog_footer, dialog_header, dialog_title, input as ui_input,
};
use topcoat::{Result, context::Cx, runtime::Event, view::*};

use super::super::{
    super::state::{TableSignals, TableState, bulk_delete_url},
    Table,
};

/// Keystroke-quiet delay before a live search input reloads the table
/// (~150-250ms): `assets/live-search.js` waits this long after the
/// last keystroke, then forwards the value through the bound transport below,
/// so typing "published" triggers one reload instead of nine. The forwarded
/// write is an ordinary signal write, so Topcoat's abort-in-flight
/// coalescing still applies to the resulting rerun.
pub(crate) const LIVE_SEARCH_DEBOUNCE_MS: u32 = 200;

/// The hidden inputs a table toolbar carries across its submit, in the order
/// given; an input whose state holds no value renders nothing.
pub(super) fn hidden_state_inputs<'a>(
    cx: &'a Cx,
    inputs: Vec<(String, Option<String>)>,
) -> BoxView<'a> {
    let fields: Vec<BoxView<'a>> = inputs
        .into_iter()
        .filter_map(|(name, value)| {
            value.map(|value| {
                view! { cx => <input type="hidden" name=(name) value=(value)> }.boxed()
            })
        })
        .collect();
    view! {
        cx =>
        for field in fields {
            (field)
        }
    }
    .boxed()
}

impl<M> Table<M> {
    /// The bulk-delete bar and its confirmation dialog, or the
    /// placeholder that keeps the chrome's node order stable when
    /// [`Self::bulk_enabled`] is off.
    ///
    /// One destructive form for the whole page: the transport is fed by the
    /// row checkboxes (`bulk.js`) and ships `,a,b,`-delimited, while on a live
    /// table the selection lives in a signal instead, so a shard
    /// rerun re-renders the transport from the selection rather than dropping
    /// it. The trigger ships enabled: the confirmation dialog gates
    /// the write and reads the selection when it opens, so an empty selection
    /// is answered by the dialog rather than by a disabled control whose state
    /// has to be kept in step across a live swap.
    ///
    /// The dialog lives *inside* the form so its `confirm` marker ships with
    /// the same payload as the selection — the confirm button is an ordinary
    /// submit of that form, and the handler refuses a POST without the marker.
    /// It renders closed and opens client-side (`showModal`) rather than
    /// through a runtime signal: the trigger is `type="button"`, so opening
    /// the dialog is not a result-set change and must not reload the table.
    pub(super) fn render_bulk_bar<'a>(
        &self,
        cx: &'a Cx,
        signals: Option<&TableSignals>,
    ) -> BoxView<'a> {
        if !self.bulk_enabled() {
            return view! { cx => <span></span> }.boxed();
        }
        let prefix = self
            .delete_prefix
            .clone()
            .expect("bulk chrome rides the delete prefix (see bulk_enabled)");
        let bulk_action = bulk_delete_url(&prefix);
        let csrf = crate::csrf::current_token(cx);
        // Stable ids so the dialog's confirm button can submit this form
        // from inside the dialog.
        let bulk_form_id = format!("{}-bulk-form", prefix.replace('/', "-"));
        let bulk_dialog_id = format!("{bulk_form_id}-confirm");
        let bulk_dialog_title_id = format!("{bulk_dialog_id}-title");
        let bulk_dialog_description_id = format!("{bulk_dialog_id}-description");
        // No visible `ids` field: the transport is fed by the row
        // checkboxes (`bulk.js`) and ships `,a,b,`-delimited. On a live
        // table the selection lives in a signal instead, so a
        // shard rerun re-renders the transport from the selection rather
        // than dropping it.
        let transport_attrs = match signals {
            Some(signals) => {
                let bulk = signals.bulk.clone();
                attributes! {
                    cx =>
                    type="hidden"
                    name="ids"
                    :value=$(bulk.get())
                    @change=$(|e: Event| bulk.set(e.target.value))
                }
            }
            None => attributes! { cx => type="hidden" name="ids" value="" },
        };
        view! {
            cx =>
            <form
                method="post"
                action=(bulk_action)
                class="flex gap-2 p-3 border-b border-border"
                data-bulk-form=""
                data-mutation-submit=""
                id=(bulk_form_id.clone())
            >
                (crate::csrf::field(cx, &csrf))
                <input (transport_attrs)>
                button(
                    variant: ButtonVariant::Destructive,
                    size: ButtonSize::Md,
                    attrs: attributes! { type="button" data-bulk-confirm-trigger="" },
                    "Bulk Delete"
                )
                // Destructive confirm: a batch is the one place a
                // misclick costs many rows, so it asks first — the same
                // alert-dialog pattern the row delete already uses.
                alert_dialog(
                    open: false,
                    attrs: attributes! {
                        id=(bulk_dialog_id.clone())
                        data-bulk-confirm-dialog=""
                        aria-labelledby=(bulk_dialog_title_id.clone())
                        aria-describedby=(bulk_dialog_description_id.clone())
                    },
                    dialog_content(
                        dialog_header(
                            dialog_title(
                                attrs: attributes! { id=(bulk_dialog_title_id.clone()) },
                                "Delete the selected records?"
                            )
                            dialog_description(
                                attrs: attributes! {
                                    id=(bulk_dialog_description_id.clone())
                                    data-bulk-confirm-description=""
                                },
                                "This action cannot be undone."
                            )
                        )
                        dialog_footer(
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
                        )
                    )
                )
            </form>
        }
        .boxed()
    }

    /// The search toolbar (GET form); live tables instead render the host
    /// input eagerly and the shard invocation in the streamed region (see
    /// [`Self::render_live_search_bar`] / [`Self::render_live_invocation`]).
    pub(super) async fn render_search_bar<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
    ) -> Result<BoxView<'a>> {
        let action = path.to_string();
        let q_display = state.search.clone().unwrap_or_default();
        let sort_hidden = state.sort.as_ref().map(|s| s.column.clone());
        let dir_hidden = state.sort.as_ref().map(|s| {
            if s.descending {
                "desc".to_string()
            } else {
                "asc".to_string()
            }
        });
        // Pre-normalized by the render seams: `state.group_by` is
        // the declared name or `None`, never an unknown value.
        let group_hidden = state.group_by.clone();
        // Clear only renders when something survives the search term; every
        // branch below projects the same URL, so one intent serves all three.
        let clear_url =
            (state.sort.is_some() || !state.filters.is_empty() || group_hidden.is_some())
                .then(|| state.without_search(path));
        let mut inputs = vec![
            ("sort".to_string(), sort_hidden),
            ("dir".to_string(), dir_hidden),
        ];
        inputs.extend(
            state
                .filters
                .iter()
                .map(|(name, value)| (crate::resource::filter_param(name), Some(value.clone()))),
        );
        inputs.push(("group_by".to_string(), group_hidden));
        let hidden = hidden_state_inputs(cx, inputs);
        Ok(view! {
            cx =>
            <form
                method="get"
                action=(action)
                class="flex flex-wrap items-center gap-2 border-b border-border p-3"
            >
                (hidden)
                ui_input(
                    attrs: attributes! {
                        type="search"
                        name="q"
                        value=(q_display)
                        placeholder="Search…"
                        aria-label="Search table"
                        class="w-64"
                    }
                )
                button(
                    variant: ButtonVariant::Secondary,
                    size: ButtonSize::Md,
                    attrs: attributes! { type="submit" },
                    "Search"
                )
                if let Some(url) = clear_url {
                    <a
                        href=(url)
                        class="text-sm text-muted-foreground hover:text-foreground"
                    >
                        "Clear"
                    </a>
                }
            </form>
        }
        .boxed())
    }

    /// Eager live-search input for live tables: the signal-backed
    /// input plus the GET form as `<noscript>` fallback. Rendered eagerly
    /// above the streamed region; the shard invocation that fills the table
    /// lives in the streamed region (`Self::render_live_invocation`) so the
    /// table can only ever render once per response.
    ///
    /// The visible input is deliberately unbound: typing stays
    /// local until it pauses for `LIVE_SEARCH_DEBOUNCE_MS`, then
    /// `assets/live-search.js` rewrites `q` in the hidden transport bound to
    /// the `query` signal and drops the cursor (a new term is a new result
    /// set). The shard re-renders in place.
    ///
    /// The panel's live list page (`panel::resource_list_live`) renders it
    /// from the request's one normalized state.
    pub(crate) async fn render_live_search_bar<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
    ) -> Result<BoxView<'a>> {
        let fallback = self.render_search_bar(cx, state, path).await?;
        let q_display = state.search.clone().unwrap_or_default();
        let query = signals.query.clone();
        Ok(view! {
            cx =>
            <div
                class="flex flex-wrap items-center gap-2 border-b border-border p-3"
                data-live-search=""
            >
                <input
                    type="search"
                    value=(q_display)
                    placeholder="Search…"
                    aria-label="Live search table"
                    class="w-64"
                    data-live-search-input=""
                    data-debounce-ms=(LIVE_SEARCH_DEBOUNCE_MS)
                >
                <input
                    type="hidden"
                    :value=$(query.get())
                    @change=$(|e: Event| query.set(e.target.value))
                    data-live-search-transport=""
                >
                <noscript>(fallback)</noscript>
            </div>
        }
        .boxed())
    }

    /// The `table_search` shard invocation filling a live table's streamed
    /// region. The signal handles travel as arguments; every
    /// tracked read inside the shard becomes a `dep` marker the browser
    /// watches, so sort/filter/pager/search changes re-render the table in
    /// place.
    pub(crate) async fn render_live_invocation<'a>(
        &self,
        cx: &'a Cx,
        path: &str,
        signals: TableSignals,
    ) -> Result<BoxView<'a>> {
        use crate::panel::table_search;

        // No snapshot here: grouping travels in the query (seeded from the page
        // state by the caller) and the shard normalizes on read.
        let live_path = path.to_string();
        let TableSignals { query, bulk } = signals;
        Ok(view! {
            cx =>
            table_search(
                path: $(live_path.clone()),
                query: $(query),
                bulk: $(bulk)
            )
        }
        .boxed())
    }
}
#[cfg(test)]
mod tests;
