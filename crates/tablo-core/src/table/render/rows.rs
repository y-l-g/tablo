//! Row projection: the page's rows as the owned views the template renders.

use std::borrow::Cow;

use tablo_ui::{ButtonSize, ButtonVariant, button, button_variants, table_cell, table_row};
use topcoat::{context::Cx, icon::icon, runtime::Event, view::*};

use super::{
    super::{GroupKey, RowActions, WiredTable},
    Frame,
    dialog::{input_trigger, write_trigger},
};
use crate::table::{
    page::TablePage,
    state::{
        TableSignals, bulk_token, delete_action_url, group_header_dom_id, row_action_url,
        row_dom_id, row_edit_url, row_view_url,
    },
};

/// What every row of one render shares: the chrome columns, the declared
/// widths (resolved once per render, the same for every row), and the
/// table's signals and write form.
pub(super) struct RowChrome {
    pub(super) with_bulk: bool,
    pub(super) with_actions: bool,
    /// Columns a group header spans: the data columns plus the chrome.
    pub(super) header_colspan: usize,
    pub(super) cell_widths: Vec<Option<Cow<'static, str>>>,
    pub(super) actions_min: Option<Cow<'static, str>>,
    pub(super) signals: TableSignals,
    /// The DOM id of the table's write form.
    pub(super) form: String,
}

/// One rendered row, keyed for the body's diff.
pub(super) struct RenderedRow<'a> {
    /// The record's primary key: stable for the record, never a loop index.
    pub(super) key: String,
    /// The group header the row opens, if any, then the row itself.
    pub(super) view: BoxView<'a>,
}

/// The keys of the rows that take a bulk checkbox.
pub(super) fn selectable_keys(rows: &[RowView<'_>]) -> Vec<String> {
    rows.iter()
        .filter(|row| row.selectable)
        .map(|row| row.key.clone())
        .collect()
}

/// Render the page's rows for the table body, carrying each group's header on its first row.
pub(super) fn render_rows<'a>(
    cx: &'a Cx,
    rows: Vec<RowView<'a>>,
    chrome: &RowChrome,
) -> Vec<RenderedRow<'a>> {
    rows.into_iter()
        .map(|row| RenderedRow {
            key: row.key.clone(),
            view: render_row(cx, row, chrome),
        })
        .collect()
}

/// One row: its group header when it opens a group, the bulk checkbox, the
/// cells, and the actions cell.
fn render_row<'a>(cx: &'a Cx, mut row: RowView<'a>, chrome: &RowChrome) -> BoxView<'a> {
    let header = row.group_header.clone().map(|header| {
        let colspan = chrome.header_colspan;
        view! {
            cx =>
            table_row(
                attrs: attributes! { id=(header.dom_id) },
                table_cell(
                    attrs: attributes! {
                        colspan=(colspan)
                        class="bg-muted/60 px-3 py-2 text-sm font-medium text-foreground"
                    },
                    (header.text)
                )
            )
        }
        .boxed()
    });
    let dom_id = row_dom_id(&row.key);
    let bulk_cell = chrome.with_bulk.then(|| {
        if row.selectable {
            let described = dom_id.clone();
            let bulk = chrome.signals.bulk.clone();
            let token = bulk_token(&row.key);
            let value = row.key.clone();
            view! {
                cx =>
                table_cell(
                    <input
                        type="checkbox"
                        value=(value)
                        aria-label="Select row"
                        aria-describedby=(described)
                        :checked=$({
                            let wire = bulk.get();
                            raw!(
                                "cx.hydrate(String(${wire}).includes(String(${token})))",
                                wire.contains(token.as_str()),
                            )
                        })
                        @change=$(|e: Event| {
                            let wire = bulk.get();
                            if e.target.checked {
                                bulk.push_str(token);
                            } else {
                                bulk.set(
                                    raw!(
                                        "cx.hydrate(String(${wire}).replaceAll(String(${token}), ',').replace(/^,+$/, ''))",
                                        wire.to_owned(),
                                    ),
                                );
                            }
                        })
                    >
                )
            }
            .boxed()
        } else {
            view! { cx => table_cell() }.boxed()
        }
    });
    let cells: Vec<(BoxView<'a>, Option<Cow<'static, str>>)> = std::mem::take(&mut row.cells)
        .into_iter()
        .zip(chrome.cell_widths.iter().cloned())
        .collect();
    let actions = chrome
        .with_actions
        .then(|| render_actions(cx, &row, chrome));
    view! {
        cx =>
        if let Some(header) = header {
            (header)
        }
        table_row(
            attrs: attributes! { id=(dom_id) },
            if let Some(cell) = bulk_cell {
                (cell)
            }
            for (cell, width) in cells {
                table_cell(
                    attrs: attributes! { class="truncate" style=(width.as_deref()) },
                    (cell)
                )
            }
            if let Some(actions) = actions {
                (actions)
            }
        )
    }
    .boxed()
}

/// The row's actions cell: its custom actions, then View, Edit and Delete, each only when the
/// row's policy allows it.
fn render_actions<'a>(cx: &'a Cx, row: &RowView<'a>, chrome: &RowChrome) -> BoxView<'a> {
    let actions_min = chrome.actions_min.clone();
    let view_url = row.view_url.clone();
    let edit_url = row.edit_url.clone();
    let link_class = button_variants(ButtonVariant::Ghost, ButtonSize::Icon);
    let edit_class = link_class.clone();
    let delete_class = link_class.clone();
    let described = row_dom_id(&row.key);
    let custom: Vec<BoxView<'a>> = row
        .custom
        .iter()
        .map(|action| {
            let RowButton {
                label,
                url,
                confirm,
                input,
            } = action;
            let mut attrs = match input {
                Some(name) => {
                    input_trigger(cx, &chrome.form, &chrome.signals, name, url.clone(), false)
                }
                None => write_trigger(
                    cx,
                    &chrome.form,
                    &chrome.signals,
                    url.clone(),
                    confirm.then_some(("Run this action?", "Confirm")),
                    false,
                ),
            };
            attrs.extend(attributes! { cx => aria-describedby=(described.clone()) });
            let label = label.clone();
            view! {
                cx =>
                button(
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    attrs: attrs,
                    (label)
                )
            }
            .boxed()
        })
        .collect();
    let delete = row.delete_action.clone().map(|action| {
        let confirm = Some(("Delete this record?", "Delete"));
        let mut attrs = write_trigger(cx, &chrome.form, &chrome.signals, action, confirm, false);
        attrs.extend(attributes! {
            cx =>
            class=(delete_class)
            aria-label="Delete"
            aria-describedby=(described.clone())
            title="Delete"
        });
        attrs
    });
    view! {
        cx =>
        table_cell(
            attrs: attributes! { style=(actions_min.as_deref()) },
            <div class="flex items-center justify-end gap-1">
                for control in custom {
                    (control)
                }
                if let Some(url) = view_url {
                    <a
                        (crate::navigation::runtime_link(cx, &url))
                        class=(link_class)
                        aria-label="View"
                        aria-describedby=(described.clone())
                        title="View"
                    >
                        icon(data: tablo_ui::icons::EYE)
                    </a>
                }
                if let Some(url) = edit_url {
                    <a
                        (crate::navigation::runtime_link(cx, &url))
                        class=(edit_class)
                        aria-label="Edit"
                        aria-describedby=(described.clone())
                        title="Edit"
                    >
                        icon(data: tablo_ui::icons::PENCIL)
                    </a>
                }
                if let Some(attrs) = delete {
                    <button (attrs)>
                        icon(
                            data: tablo_ui::icons::TRASH,
                            attrs: attributes! { class="text-destructive" }
                        )
                    </button>
                }
            </div>
        )
    }
    .boxed()
}

/// Precompute per-row presentation for the table body in owned data the lazy view captures.
pub(super) struct RowView<'a> {
    /// The record's primary key, driving keyed diffs, DOM ids, URLs and bulk values.
    key: String,
    /// One rendered cell per column, in column order.
    cells: Vec<BoxView<'a>>,
    view_url: Option<String>,
    edit_url: Option<String>,
    /// The row's delete POST target, which the confirmation dialog posts.
    delete_action: Option<String>,
    /// The custom row actions this record allows.
    custom: Vec<RowButton>,
    /// Whether the row renders a bulk checkbox.
    selectable: bool,
    /// The row's group label, when `?group_by=` names the declared group.
    group: Option<String>,
    /// The header this row renders above itself, `Some` only on the first row
    /// of its group.
    group_header: Option<GroupHeader>,
}

/// One custom action's button in a row.
struct RowButton {
    label: String,
    /// The action's POST target for the row's record.
    url: String,
    /// Whether it asks first through the confirmation dialog.
    confirm: bool,
    /// The action's name when it asks for input, which its dialog then does.
    input: Option<&'static str>,
}

/// One page-local group header: the label with its page-local count and the stable DOM id the
/// injected header row carries.
#[derive(Clone)]
struct GroupHeader {
    /// `"{label} ({n} on this page)"`.
    text: String,
    dom_id: String,
}

impl<M> WiredTable<M> {
    /// Whether `row`, whose policy allows `actions`, takes a bulk checkbox: bulk delete
    /// allows deleting it, or a bulk custom action allows it.
    fn selectable(&self, frame: &Frame<'_>, row: &M, actions: RowActions) -> bool {
        (frame.bulk_delete && actions.delete)
            || self
                .bulk_custom_actions()
                .any(|action| (action.allowed)(row))
    }

    /// Project the loaded page into the row presentation the template renders, wired as
    /// `frame` says.
    pub(super) fn row_views<'a>(
        &self,
        cx: &'a Cx,
        frame: &Frame<'_>,
        page: &TablePage<M>,
        group_key: Option<&GroupKey<M>>,
    ) -> Vec<RowView<'a>>
    where
        M: toasty::schema::Model,
    {
        let gated = frame.delete_prefix.is_some()
            || frame.edit_prefix.is_some()
            || frame.view_prefix.is_some();
        let mut row_data: Vec<RowView<'a>> = page
            .rows
            .iter()
            .map(|row| {
                let key = self.key_of(row);
                let actions = if gated {
                    self.actions_for(row)
                } else {
                    RowActions::ALL
                };
                let cells: Vec<BoxView<'a>> =
                    self.columns.iter().map(|col| col.cell(cx, row)).collect();
                let edit_url = frame
                    .edit_prefix
                    .filter(|_| actions.edit)
                    .map(|prefix| frame.action_url(row_edit_url(prefix, &key)));
                let view_url = frame
                    .view_prefix
                    .filter(|_| actions.view)
                    .map(|prefix| row_view_url(prefix, &key));
                let delete_action = frame
                    .delete_prefix
                    .filter(|_| actions.delete)
                    .map(|prefix| frame.action_url(delete_action_url(prefix, &key)));
                let custom = frame
                    .actions_prefix
                    .map(|prefix| {
                        self.row_custom_actions()
                            .filter(|action| (action.allowed)(row))
                            .map(|action| {
                                let url = row_action_url(prefix, &key, action.name);
                                let input = action.input.map(|_| action.name);
                                RowButton {
                                    label: action.label.clone(),
                                    url: frame.action_url(url),
                                    confirm: action.confirm && input.is_none(),
                                    input,
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let selectable = frame.bulk_enabled() && self.selectable(frame, row, actions);
                RowView {
                    key,
                    cells,
                    view_url,
                    edit_url,
                    delete_action,
                    custom,
                    selectable,
                    group: group_key.map(|group| group(row)),
                    group_header: None,
                }
            })
            .collect();
        if group_key.is_some() {
            row_data.sort_by(|a, b| a.group.cmp(&b.group));
            let mut start = 0;
            while start < row_data.len() {
                let label = row_data[start].group.clone().unwrap_or_default();
                let mut end = start;
                while end < row_data.len() && row_data[end].group.as_deref() == Some(label.as_str())
                {
                    end += 1;
                }
                row_data[start].group_header = Some(GroupHeader {
                    dom_id: group_header_dom_id(&label),
                    text: format!("{label} ({} on this page)", end - start),
                });
                start = end;
            }
        }
        row_data
    }
}
