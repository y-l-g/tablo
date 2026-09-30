//! The search bar, live-search bar, and bulk-action bar.

use tablo_ui::{ButtonSize, ButtonVariant, button, input as ui_input};
use topcoat::{Result, context::Cx, icon::icon, runtime::Event, view::*};

use super::{
    super::{
        super::state::{TableSignals, TableState, bulk_delete_url},
        Table,
    },
    BAR_CLASS, QUIET_LINK_CLASS, SEARCH_FIELD_CLASS, SEARCH_FORM_CLASS, SEARCH_ICON_CLASS,
    dialog::{ConfirmDialog, chrome_dom_id, confirm_controls, confirm_dialog},
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
        let bulk_action = self.action_url(bulk_delete_url(&prefix));
        let csrf = crate::csrf::current_token(cx);
        // Stable ids so the dialog's confirm button can submit this form
        // from inside the dialog.
        let bulk_form_id = chrome_dom_id(&prefix, "bulk-form");
        // Destructive confirm: a batch is the one place a misclick costs many
        // rows, so it asks first — the same alert dialog the row delete uses.
        // It sits inside the bulk form, so its controls submit that form.
        let confirm = confirm_dialog(
            cx,
            ConfirmDialog {
                id: chrome_dom_id(&prefix, "bulk-form-confirm"),
                open: false,
                title: "Delete the selected records?",
                attrs: attributes! { cx => data-bulk-confirm-dialog="" },
                description_attrs: attributes! { cx => data-bulk-confirm-description="" },
                footer: confirm_controls(cx),
            },
        );
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
                class="ml-auto flex items-center gap-2"
                data-bulk-form=""
                data-mutation-submit=""
                id=(bulk_form_id.clone())
            >
                (crate::csrf::field(cx, &csrf))
                <input (transport_attrs)>
                button(
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Md,
                    attrs: attributes! { type="button" data-bulk-confirm-trigger="" },
                    icon(
                        data: tablo_ui::icons::TRASH,
                        attrs: attributes! { class="text-destructive" }
                    )
                    "Delete selected"
                )
                (confirm)
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
            (state.param("sort"), sort_hidden),
            (state.param("dir"), dir_hidden),
        ];
        inputs.extend(
            state
                .filters
                .iter()
                .map(|(name, value)| (state.filter_param(name), Some(value.clone()))),
        );
        inputs.push((state.param("group_by"), group_hidden));
        let hidden = hidden_state_inputs(cx, inputs);
        let search_name = state.param("q");
        Ok(view! {
            cx =>
            <form method="get" action=(action) class=(SEARCH_FORM_CLASS)>
                (hidden)
                <div class=(SEARCH_FIELD_CLASS)>
                    icon(
                        data: tablo_ui::icons::SEARCH,
                        attrs: attributes! { class=(SEARCH_ICON_CLASS) }
                    )
                    ui_input(
                        attrs: attributes! {
                            type="search"
                            name=(search_name)
                            value=(q_display)
                            placeholder="Search…"
                            aria-label="Search table"
                            class="pl-8"
                        }
                    )
                </div>
                button(
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Md,
                    attrs: attributes! { type="submit" },
                    "Search"
                )
                if let Some(url) = clear_url {
                    <a href=(url) class=(QUIET_LINK_CLASS)>"Clear"</a>
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
        // The query prefix a relation's parameters carry (`comments.`), so
        // `live-search.js` rewrites the table's own `q` and cursor instead of
        // the list's. Absent on a page-owned list, whose parameters are bare.
        let query_prefix = state.prefix.as_deref().map(|prefix| format!("{prefix}."));
        Ok(view! {
            cx =>
            <div class=(BAR_CLASS) data-live-search="" data-query-prefix=(query_prefix)>
                <div class=(SEARCH_FIELD_CLASS)>
                    icon(
                        data: tablo_ui::icons::SEARCH,
                        attrs: attributes! { class=(SEARCH_ICON_CLASS) }
                    )
                    ui_input(
                        attrs: attributes! {
                            type="search"
                            value=(q_display)
                            placeholder="Search…"
                            aria-label="Live search table"
                            class="pl-8"
                            data-live-search-input=""
                            data-debounce-ms=(LIVE_SEARCH_DEBOUNCE_MS)
                        }
                    )
                </div>
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

        // No snapshot here: grouping travels in the query (seeded from the
        // request's query by the caller) and the shard normalizes on read.
        let live_path = path.to_string();
        let TableSignals { query, bulk } = signals;
        Ok(view! {
            cx =>
            table_search(path: $(live_path.clone()), query: $(query), bulk: $(bulk))
        }
        .boxed())
    }

    /// The `table_relation_search` shard invocation filling a live relation
    /// table's streamed region. The signal handles travel as arguments; every
    /// tracked read inside the shard becomes a `dep` marker the browser
    /// watches, so sort/filter/pager/search changes re-render the table in
    /// place.
    pub(crate) async fn render_live_relation_invocation<'a>(
        &self,
        cx: &'a Cx,
        parent: &str,
        child: &str,
        ctx: crate::panel::RelationRequest,
        signals: TableSignals,
    ) -> Result<BoxView<'a>> {
        use crate::panel::table_relation_search;

        // No snapshot here: grouping travels in the query (seeded from the
        // request's query by the caller) and the shard normalizes on read.
        // Slugs never carry `/`, so the pair and the seed travel as one wire
        // arg, split off the front by the shard.
        let crate::panel::RelationRequest {
            seed,
            page: live_page,
            read_only,
        } = ctx;
        let scope = format!("{parent}/{child}/{seed}");
        let TableSignals { query, bulk } = signals;
        Ok(view! {
            cx =>
            table_relation_search(
                scope: $(scope.clone()),
                page: $(live_page.clone()),
                read_only: $(read_only),
                query: $(query),
                bulk: $(bulk)
            )
        }
        .boxed())
    }
}
#[cfg(test)]
mod tests;
