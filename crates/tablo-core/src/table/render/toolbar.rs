//! The search bar, live-search bar, and bulk-action bar.

use tablo_ui::{ButtonSize, ButtonVariant, button, input as ui_input};
use topcoat::{Result, context::Cx, icon::icon, runtime::Event, view::*};

use super::{
    super::Table,
    BAR_CLASS, QUIET_LINK_CLASS, SEARCH_FIELD_CLASS, SEARCH_FORM_CLASS, SEARCH_ICON_CLASS,
    dialog::{ConfirmDialog, chrome_dom_id, confirm_controls, confirm_dialog},
};
use crate::table::state::{TableSignals, TableState, bulk_action_url, bulk_delete_url};

/// Delay a live search input waits after the last keystroke before reloading the table.
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
    /// Render the bulk bar, or a placeholder keeping node order stable when bulk actions are off.
    pub(super) fn render_bulk_bar<'a>(
        &self,
        cx: &'a Cx,
        signals: Option<&TableSignals>,
    ) -> BoxView<'a> {
        if !self.bulk_enabled() {
            return view! { cx => <span></span> }.boxed();
        }
        let delete_prefix = self
            .delete_prefix
            .clone()
            .filter(|_| self.bulk_delete_enabled());
        let prefix = delete_prefix
            .clone()
            .or_else(|| self.actions_prefix.clone())
            .expect("bulk chrome rides the delete or the actions prefix (see bulk_enabled)");
        let custom: Vec<(String, String, bool)> = self
            .bulk_custom_actions()
            .map(|action| {
                (
                    action.label.clone(),
                    self.action_url(bulk_action_url(&prefix, action.name)),
                    action.confirm,
                )
            })
            .collect();
        let bulk_action = match &delete_prefix {
            Some(prefix) => self.action_url(bulk_delete_url(prefix)),
            None => custom
                .first()
                .map(|(_, url, _)| url.clone())
                .unwrap_or_default(),
        };
        let csrf = crate::csrf::current_token(cx);
        let bulk_form_id = chrome_dom_id(&prefix, "bulk-form");
        let confirm = delete_prefix.as_ref().map(|_| {
            confirm_dialog(
                cx,
                ConfirmDialog {
                    id: chrome_dom_id(&prefix, "bulk-form-confirm"),
                    open: false,
                    title: "Delete the selected records?",
                    attrs: attributes! { cx => data-bulk-confirm-dialog="" },
                    description_attrs: attributes! { cx => data-bulk-confirm-description="" },
                    footer: confirm_controls(cx, "Delete"),
                },
            )
        });
        let with_delete = confirm.is_some();
        let action_confirm = self
            .bulk_custom_actions()
            .any(|action| action.confirm)
            .then(|| {
                confirm_dialog(
                    cx,
                    ConfirmDialog {
                        id: chrome_dom_id(&prefix, "bulk-action-confirm"),
                        open: false,
                        title: "Run this action?",
                        attrs: attributes! { cx => data-bulk-action-confirm-dialog="" },
                        description_attrs: attributes! { cx => data-bulk-confirm-description="" },
                        footer: confirm_controls(cx, "Confirm"),
                    },
                )
            });
        let custom_buttons: Vec<BoxView<'a>> = custom
            .into_iter()
            .map(|(label, url, confirm)| {
                if confirm {
                    return view! {
                        cx =>
                        button(
                            variant: ButtonVariant::Outline,
                            size: ButtonSize::Md,
                            attrs: attributes! {
                                type="button"
                                data-bulk-action-confirm-trigger=""
                                data-bulk-action-confirm-action=(url)
                            },
                            (label)
                        )
                    }
                    .boxed();
                }
                view! {
                    cx =>
                    button(
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Md,
                        attrs: attributes! { type="submit" formaction=(url) data-bulk-action="" },
                        (label)
                    )
                }
                .boxed()
            })
            .collect();
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
                for custom_button in custom_buttons {
                    (custom_button)
                }
                if with_delete {
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
                }
                if let Some(confirm) = confirm {
                    (confirm)
                }
                if let Some(confirm) = action_confirm {
                    (confirm)
                }
            </form>
        }
        .boxed()
    }

    /// Render the search toolbar as a GET form.
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
        let group_hidden = state.group_by.clone();
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

    /// Render the eager live-search input for live tables with the GET form as fallback.
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
}
#[cfg(test)]
mod tests;
