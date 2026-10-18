//! The toolbar above the table: the search field, the filter controls, and the bulk actions.

use tablo_ui::{
    ButtonSize, ButtonVariant, FloatingAlign, button, button_variants, checkbox, dropdown_menu,
    dropdown_menu_content, dropdown_menu_label, dropdown_menu_separator, dropdown_menu_trigger,
    input as ui_input,
};
use topcoat::{
    Result,
    context::Cx,
    icon::icon,
    runtime::{Event, EventHandlerFn, Expr, expr},
    view::*,
};

use super::{
    BAR_CLASS, ColumnPicker, Frame, SEARCH_FIELD_CLASS, SEARCH_ICON_CLASS,
    dialog::{input_trigger, write_trigger},
    table_dom_id,
};
use crate::{
    panel::{
        list::{COLUMN_FIELD, RESET_FIELD, TableSignals},
        url::{bulk_action_url, bulk_delete_url},
    },
    table::TableState,
};

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
pub(super) fn rerun_with(signals: &TableSignals, form: &str) -> Expr<impl EventHandlerFn + use<>> {
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
        let (picker, picker_form) = self
            .picker
            .as_ref()
            .map(|picker| self.render_picker(cx, state, picker))
            .unzip();
        if !show_search && !show_filters && !with_bulk && picker.is_none() {
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
                (
                    state.param("per_page"),
                    state.per_page.map(|n| n.to_string()),
                ),
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
                if search.is_some() || bulk.is_some() || picker.is_some() {
                    <div class=(BAR_CLASS)>
                        if let Some(search) = search {
                            (search)
                        }
                        <div class="ml-auto flex items-center gap-2">
                            if let Some(bulk) = bulk {
                                (bulk)
                            }
                            if let Some(picker) = picker {
                                (picker)
                            }
                        </div>
                    </div>
                }
                if let Some(filters) = filters {
                    (filters)
                }
            </form>
            // Forms do not nest: the picker's controls name this form from inside the toolbar's.
            if let Some(form) = picker_form {
                (form)
            }
        }
        .boxed())
    }

    /// Render the column picker: a menu checking each toggleable column the reader's choice shows,
    /// whose Apply posts the checked ones and whose Reset restores the defaults, and the POST form
    /// its controls name, which renders outside the toolbar's.
    fn render_picker<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        picker: &ColumnPicker<'_>,
    ) -> (BoxView<'a>, BoxView<'a>) {
        let menu = table_dom_id(state, "columns");
        let form = table_dom_id(state, "columns-form");
        let boxes: Vec<BoxView<'a>> = picker
            .columns
            .iter()
            .map(|column| {
                let name = column.name.to_string();
                let label = column.label.to_string();
                let checked = column.shown.then_some("");
                let form = form.clone();
                view! {
                    cx =>
                    <label
                        class="flex cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-foreground/5"
                    >
                        checkbox(
                            attrs: attributes! {
                                name=(COLUMN_FIELD)
                                value=(name)
                                form=(form)
                                checked=(checked)
                            }
                        )
                        (label)
                    </label>
                }
                .boxed()
            })
            .collect();
        let trigger = button_variants(ButtonVariant::Outline, ButtonSize::Md);
        let (reset_form, apply_form) = (form.clone(), form.clone());
        let menu_view = view! {
            cx =>
            dropdown_menu(
                dropdown_menu_trigger(
                    target: &menu,
                    attrs: attributes! { class=(trigger) },
                    icon(data: tablo_ui::icons::COLUMNS_3)
                    "Columns"
                )
                dropdown_menu_content(
                    id: &menu,
                    align: FloatingAlign::End,
                    // The boxes belong to the picker's form but sit inside the toolbar's, whose
                    // `change` handler would rerun the list on each tick and drop its page.
                    attrs: attributes! {
                        class="min-w-56"
                        @change=$(|e: Event| e.stop_propagation())
                    },
                    dropdown_menu_label("Columns")
                    for item in boxes {
                        (item)
                    }
                    dropdown_menu_separator()
                    <div class="flex justify-end gap-2 p-1">
                        button(
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Sm,
                            attrs: attributes! {
                                type="submit"
                                form=(reset_form)
                                name=(RESET_FIELD)
                                value="1"
                            },
                            "Reset"
                        )
                        button(
                            variant: ButtonVariant::Primary,
                            size: ButtonSize::Sm,
                            attrs: attributes! { type="submit" form=(apply_form) },
                            "Apply"
                        )
                    </div>
                )
            )
        }
        .boxed();
        let action = self.action_url(picker.url.to_string());
        let csrf = crate::csrf::current_token(cx);
        let form_view = view! {
            cx =>
            <form id=(form) method="post" action=(action)>
                (crate::csrf::field(cx, &csrf))
            </form>
        }
        .boxed();
        (menu_view, form_view)
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
        // A relation table's bulk actions post to its owner, not to the related resource's list
        // that `delete_prefix` names.
        let actions_list = self.actions_prefix.unwrap_or(list.as_str());
        let form = table_dom_id(state, "writes");
        let mut buttons: Vec<BoxView<'a>> = self
            .bulk_actions
            .iter()
            .map(|action| {
                let url = self.action_url(bulk_action_url(actions_list, action.name));
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
            <div class="flex items-center gap-2">
                for control in buttons {
                    (control)
                }
            </div>
        }
        .boxed()
    }
}
