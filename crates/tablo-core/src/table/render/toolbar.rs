//! The toolbar above the table: the search field, the filter controls, and the bulk actions.

use tablo_ui::{ButtonSize, ButtonVariant, button, input as ui_input};
use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    runtime::{Event, EventHandlerFn, Expr, expr},
    view::*,
};

use super::{
    BAR_CLASS, Frame, SEARCH_FIELD_CLASS, SEARCH_ICON_CLASS,
    dialog::{input_trigger, write_trigger},
    table_dom_id,
};
use crate::table::state::{TableSignals, TableState, bulk_action_url, bulk_delete_url};

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

/// Writes the toolbar form's fields to the table's `query` signal, which reruns the page. The
/// form's fields spell the list state as its URL does, without the cursor: a new search or
/// filter is a new result set. A submit stays on the page; `change` cannot be cancelled, so the
/// same handler serves it. An unchanged query, such as the search field's `change` as it loses
/// focus, reruns nothing.
fn rerun_with(signals: &TableSignals, form: &str) -> Expr<impl EventHandlerFn + use<>> {
    let query = signals.query.clone();
    let form = form.to_string();
    expr!(|e: Event| {
        e.prevent_default();
        let next = raw!(
            "cx.hydrate(new URLSearchParams(new FormData(document.getElementById(String(${form})))).toString())",
            form.clone()
        );
        if next != query.get() {
            query.set(next);
        }
    })
}

impl Frame<'_> {
    /// Render the toolbar: a GET form holding the search field and the filter `controls`, which
    /// rewrites the table's query as they change, and the bulk actions beside the search. Typing
    /// submits the form once the reader pauses; Enter submits it, with or without JavaScript.
    pub(super) async fn render_toolbar<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        signals: &TableSignals,
        controls: Vec<BoxView<'a>>,
    ) -> Result<BoxView<'a>> {
        let show_search = self.search;
        let show_filters = self.filter_bar;
        let with_bulk = self.bulk_enabled();
        if !show_search && !show_filters && !with_bulk {
            return Ok(().boxed());
        }
        let form = table_dom_id(state, "toolbar");
        let sort_hidden = state.sort.as_ref().map(|s| s.column.clone());
        let dir_hidden = state
            .sort
            .as_ref()
            .map(|s| if s.descending { "desc" } else { "asc" }.to_string());
        let hidden = hidden_state_inputs(
            cx,
            vec![
                (state.param("sort"), sort_hidden),
                (state.param("dir"), dir_hidden),
                (state.param("group_by"), state.group_by.clone()),
            ],
        );
        let search = show_search.then(|| {
            let name = state.param("q");
            let value = state.search.clone().unwrap_or_default();
            let form = form.clone();
            view! {
                cx =>
                <div class=(SEARCH_FIELD_CLASS)>
                    icon(
                        data: tablo_ui::icons::SEARCH,
                        attrs: attributes! { class=(SEARCH_ICON_CLASS) }
                    )
                    ui_input(
                        attrs: attributes! {
                            type="search"
                            name=(name)
                            value=(value)
                            placeholder="Search…"
                            aria-label="Search table"
                            class="pl-8"
                            @input=$(|_e: Event| raw!(
                                    "((f) => { clearTimeout(f.tabloSearch); f.tabloSearch = setTimeout(() => f.requestSubmit(), 200) })(document.getElementById(String(${form})))",
                                    (),
                                ))
                        }
                    )
                </div>
            }
            .boxed()
        });
        let bulk = with_bulk.then(|| self.render_bulk_bar(cx, state, signals));
        let filters = if show_filters {
            Some(self.render_filter_controls(cx, state, path, signals, controls))
        } else {
            None
        };
        let (on_change, on_submit) = (rerun_with(signals, &form), rerun_with(signals, &form));
        let action = path.to_string();
        Ok(view! {
            cx =>
            <form
                id=(form)
                method="get"
                action=(action)
                @change=(on_change)
                @submit=(on_submit)
            >
                (hidden)
                <button type="submit" class="sr-only" tabindex="-1">"Search"</button>
                if search.is_some() || bulk.is_some() {
                    <div class=(BAR_CLASS)>
                        if let Some(search) = search {
                            (search)
                        }
                        if let Some(bulk) = bulk {
                            (bulk)
                        }
                    </div>
                }
                if let Some(filters) = filters {
                    (filters)
                }
            </form>
        }
        .boxed())
    }

    /// Render the bulk actions: each submits the table's write form with the selection, and is
    /// disabled while nothing is selected.
    fn render_bulk_bar<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        signals: &TableSignals,
    ) -> BoxView<'a> {
        let list = self
            .delete_prefix
            .or(self.actions_prefix)
            .expect("bulk chrome rides the delete or the actions prefix (see bulk_enabled)")
            .to_string();
        let form = table_dom_id(state, "writes");
        let mut buttons: Vec<BoxView<'a>> = self
            .bulk_actions
            .iter()
            .map(|action| {
                let url = self.action_url(bulk_action_url(&list, action.name));
                let mut attrs = if action.input {
                    input_trigger(cx, &form, signals, action.name, url, true)
                } else {
                    let confirm = action.confirm.then_some(("Run this action?", "Confirm"));
                    write_trigger(cx, &form, signals, url, confirm, true)
                };
                let bulk = signals.bulk.clone();
                attrs.extend(attributes! { cx => :disabled=$(bulk.get().is_empty()) });
                let label = action.label.to_string();
                view! {
                    cx =>
                    button(
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Md,
                        attrs: attrs,
                        (label)
                    )
                }
                .boxed()
            })
            .collect();
        if self.bulk_delete {
            let url = self.action_url(bulk_delete_url(&list));
            let confirm = Some(("Delete the selected records?", "Delete"));
            let mut attrs = write_trigger(cx, &form, signals, url, confirm, true);
            let bulk = signals.bulk.clone();
            attrs.extend(attributes! { cx => :disabled=$(bulk.get().is_empty()) });
            buttons.push(
                view! {
                    cx =>
                    button(
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Md,
                        attrs: attrs,
                        icon(
                            data: tablo_ui::icons::TRASH,
                            attrs: attributes! { class="text-destructive" }
                        )
                        "Delete selected"
                    )
                }
                .boxed(),
            );
        }
        view! {
            cx =>
            <div class="ml-auto flex items-center gap-2">
                for control in buttons {
                    (control)
                }
            </div>
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests;
