//! The search bar, live-search bar, and bulk-action bar.

use tablo_ui::{
    ButtonSize, ButtonVariant, alert_dialog, button, dialog_content, dialog_description,
    dialog_footer, dialog_header, dialog_title, input as ui_input,
};
use topcoat::{Result, context::Cx, runtime::Event, view::*};

use super::super::{
    super::state::{TableSignals, TableState, bulk_delete_url},
    NormalizedState, Table,
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
    inputs: Vec<(&'static str, Option<String>)>,
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
        let filters_hidden = state.filters_param();
        // Pre-normalized by the render seams: `state.group_by` is
        // the declared name or `None`, never an unknown value.
        let group_hidden = state.group_by.clone();
        // Clear only renders when something survives the search term; every
        // branch below projects the same URL, so one intent serves all three.
        let clear_url =
            (state.sort.is_some() || filters_hidden.is_some() || group_hidden.is_some())
                .then(|| state.without_search(path));
        let hidden = hidden_state_inputs(
            cx,
            vec![
                ("sort", sort_hidden),
                ("dir", dir_hidden),
                ("filters", filters_hidden),
                ("group_by", group_hidden),
            ],
        );
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
    /// `assets/live-search.js` forwards the value through the bound hidden
    /// transport, whose `@change` writes `q` and clears the cursors (a new
    /// term is a new result set). The shard re-renders in place.
    ///
    /// Public so a page owning its own signals can render the same toolbar
    /// above its own shard (the showcase demos, GH #154 §2); resource lists
    /// reach it through `panel::resource_list_live`.
    pub async fn render_live_search_bar<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
    ) -> Result<BoxView<'a>> {
        // Called directly with raw state (panel live page, showcase demos):
        // normalize for the `<noscript>` fallback links.
        self.render_live_search_bar_normalized(cx, &self.normalize_state(state), path, signals)
            .await
    }

    /// [`Self::render_live_search_bar`] with the state already normalized
    /// the panel's live page renders the toolbar from the request's
    /// one normalized state.
    pub(crate) async fn render_live_search_bar_normalized<'a>(
        &self,
        cx: &'a Cx,
        state: &NormalizedState,
        path: &str,
        signals: &TableSignals,
    ) -> Result<BoxView<'a>> {
        let fallback = self.render_search_bar(cx, state, path).await?;
        let q_display = state.search.clone().unwrap_or_default();
        let q = signals.q.clone();
        let cursor = signals.cursor.clone();
        let none = crate::resource::cursor_none();
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
                    :value=$(q.get())
                    @change=$(|e: Event| {
                        q.set(e.target.value);
                        cursor.set(none.clone());
                    })
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

        // No snapshot here: grouping travels as the `group_by`
        // live signal (seeded from the page state by the caller) and the
        // shard normalizes on read.
        let live_path = path.to_string();
        let TableSignals {
            q,
            filters,
            sort,
            dir,
            cursor,
            group_by,
            bulk,
        } = signals;
        Ok(view! {
            cx =>
            table_search(
                path: $(live_path.clone()),
                q: $(q),
                filters: $(filters),
                sort: $(sort),
                dir: $(dir),
                cursor: $(cursor),
                group_by: $(group_by),
                bulk: $(bulk)
            )
        }
        .boxed())
    }
}
#[cfg(test)]
mod tests {
    use topcoat::context::CxTestBuilder;

    use super::{super::core::tests::User, *};
    use crate::{TablePage, TextColumn};

    #[test]
    fn live_search_debounce_sits_in_the_locked_band() {
        // GH #172 decision 4: ~150-250ms at the `@input` handler. The
        // markup test below pins the rendered value; this pins the range.
        assert!(
            (150..=250).contains(&LIVE_SEARCH_DEBOUNCE_MS),
            "debounce must sit in the 150-250ms band, got {LIVE_SEARCH_DEBOUNCE_MS}"
        );
    }

    #[tokio::test]
    async fn bulk_checkboxes_render_with_keys_and_select_all() {
        let cx = CxTestBuilder::new().build();
        let bulk_table = Table::<User>::r#for(&cx)
            .key(|u| u.id.to_string())
            .columns(TextColumn::r#for(User::fields().name(), |u| u.name.clone()))
            .with_delete("/admin/users".to_string())
            .with_bulk_delete(true);
        let rows = vec![
            User {
                id: uuid::Uuid::new_v4(),
                name: "Ada".to_string(),
            },
            User {
                id: uuid::Uuid::new_v4(),
                name: "Bob".to_string(),
            },
        ];
        let page: TablePage<User> = rows.clone().into();
        let html = bulk_table
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        // Per-row checkbox carries the record key; header select-all present.
        for row in &rows {
            assert!(
                html.contains(&format!("value=\"{}\"", row.id)),
                "missing checkbox value for {} in {html}",
                row.id
            );
        }
        assert!(
            html.contains("data-row-select"),
            "missing row checkbox marker in {html}"
        );
        assert!(
            html.contains("data-bulk-select-all"),
            "missing select-all in {html}"
        );
        // Bulk form keeps the hidden `ids` transport and a submit
        // that ships disabled until `bulk.js` sees a checked row.
        assert!(
            html.contains("data-bulk-form"),
            "missing bulk form in {html}"
        );
        assert!(
            html.contains("name=\"ids\"") && !html.contains("ids comma-separated"),
            "missing hidden ids transport in {html}"
        );
        // the destructive write is gated by the confirmation dialog
        // rather than by a disabled control — the trigger opens it, and the
        // dialog's own submit carries `confirm=1` inside the same form.
        assert!(
            html.contains("data-bulk-confirm-trigger"),
            "missing the bulk confirm trigger in {html}"
        );
        assert!(
            html.contains("data-bulk-confirm-dialog"),
            "missing the bulk confirm dialog in {html}"
        );
        assert!(
            html.contains("name=\"confirm\"") && html.contains("value=\"1\""),
            "the dialog must carry the confirm marker in {html}"
        );
        // Rendered closed: it is opened client-side so that opening it is not
        // a result-set change. Matched as `open="` rather than `open`, because
        // the dialog's class carries Tailwind's `open:` state variants.
        let dialog_at = html
            .find("data-bulk-confirm-dialog")
            .expect("the dialog marker");
        let dialog_tag_start = html[..dialog_at].rfind("<dialog").expect("its <dialog>");
        let dialog_tag_end = html[dialog_tag_start..].find('>').expect("the tag's end");
        let dialog_tag = &html[dialog_tag_start..dialog_tag_start + dialog_tag_end];
        assert!(
            !dialog_tag.contains("open=\""),
            "the bulk confirm dialog must render closed, got {dialog_tag}"
        );
        // `dialog.js` refuses to dismiss an alert dialog on a backdrop
        // click, so the role is the contract that keeps the confirm dialog
        // waiting for an answer rather than treating a stray click as one.
        assert!(
            dialog_tag.contains("role=\"alertdialog\""),
            "the bulk confirm dialog must be an alert dialog, got {dialog_tag}"
        );
        // The dialog is the decision, not decoration: it asks, and
        // it offers a way out that is not deleting. Absorbed from the showcase
        // duplicate so the one test that owns bulk chrome owns all
        // of it.
        assert!(
            html.contains("Delete the selected records?"),
            "the dialog must ask before it deletes, got {html}"
        );
        assert!(
            html.contains("data-dialog-close"),
            "the dialog needs a way out that is not deleting, got {html}"
        );
        // The confirm control rides inside the bulk form, so the confirmed
        // submit ships it with the same payload as the selection: `bulk.js`
        // closes over `trigger.closest('form[data-bulk-form]')`, so a dialog
        // outside the form would be decoration a crafted request skips.
        let form_at = html.find("data-bulk-form").expect("the bulk form");
        assert!(
            form_at < dialog_at,
            "the dialog must live inside the bulk form, got {html}"
        );
        assert!(
            html.contains("Bulk Delete"),
            "missing bulk button in {html}"
        );
        assert!(
            html.contains("data-table-root"),
            "missing table root scope in {html}"
        );

        // Without bulk: no checkboxes, no bulk form.
        let plain = Table::<User>::r#for(&cx)
            .key(|u| u.id.to_string())
            .columns(TextColumn::r#for(User::fields().name(), |u| u.name.clone()));
        let page: TablePage<User> = rows.into();
        let html = plain
            .render(&cx, page)
            .await
            .unwrap()
            .single()
            .await
            .unwrap()
            .render(&cx);
        assert!(
            !html.contains("data-row-select") && !html.contains("data-bulk-form"),
            "plain table must not render bulk chrome in {html}"
        );
    }
}
