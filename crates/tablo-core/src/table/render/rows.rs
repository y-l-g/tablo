//! Row projection: the page's rows as the owned views the template renders.

use std::borrow::Cow;

use tablo_ui::{ButtonSize, ButtonVariant, button, button_variants, table_cell, table_row};
use topcoat::{context::Cx, icon::icon, view::*};

use super::super::{GroupKey, RowActions, Table};
use crate::table::{
    page::TablePage,
    state::{
        TableState, delete_action_url, group_header_dom_id, row_action_url, row_dom_id,
        row_edit_url, row_view_url,
    },
};

/// What every row of one render shares: the chrome columns, the declared
/// widths (resolved once per render, the same for every row), and the one
/// delete dialog every row control opens.
pub(super) struct RowChrome {
    pub(super) with_bulk: bool,
    pub(super) with_actions: bool,
    /// Columns a group header spans: the data columns plus the chrome.
    pub(super) header_colspan: usize,
    pub(super) cell_widths: Vec<Option<Cow<'static, str>>>,
    pub(super) actions_min: Option<Cow<'static, str>>,
    pub(super) delete_dialog_id: String,
    pub(super) action_dialog_id: String,
}

/// One rendered row, keyed for the body's diff.
pub(super) struct RenderedRow<'a> {
    /// The record's primary key: stable for the record, never a loop index.
    pub(super) key: String,
    /// The group header the row opens, if any, then the row itself.
    pub(super) view: BoxView<'a>,
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
            let value = row.key.clone();
            let described = dom_id.clone();
            view! {
                cx =>
                table_cell(
                    <input
                        type="checkbox"
                        value=(value)
                        aria-label="Select row"
                        aria-describedby=(described)
                        data-row-select=""
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

/// The row's actions cell: View, Edit and Delete, each only when the row's
/// policy allows it.
fn render_actions<'a>(cx: &'a Cx, row: &RowView<'a>, chrome: &RowChrome) -> BoxView<'a> {
    let actions_min = chrome.actions_min.clone();
    let view_url = row.view_url.clone();
    let edit_url = row.edit_url.clone();
    let delete = row
        .delete_url
        .clone()
        .zip(row.delete_action.clone())
        .map(|(url, action)| (url, action, chrome.delete_dialog_id.clone()));
    let link_class = button_variants(ButtonVariant::Ghost, ButtonSize::Icon);
    let edit_class = link_class.clone();
    let delete_class = link_class.clone();
    let csrf = (!row.custom.is_empty()).then(|| crate::csrf::current_token(cx));
    let dialog = chrome.action_dialog_id.clone();
    let described = row_dom_id(&row.key);
    let custom: Vec<BoxView<'a>> = row
        .custom
        .iter()
        .map(|(label, url, confirm)| {
            let label = label.clone();
            let url = url.clone();
            if *confirm {
                // A confirmatory action borrows the row-delete dialog
                // mechanism: the trigger names the dialog and carries its
                // POST target, so no new script is needed.
                let trigger = dialog.clone();
                let described_by = described.clone();
                return view! {
                    cx =>
                    button(
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        attrs: attributes! {
                            type="button"
                            data-row-delete-trigger=(trigger)
                            data-row-delete-action=(url)
                            aria-describedby=(described_by)
                        },
                        (label)
                    )
                }
                .boxed();
            }
            let token = csrf.clone().unwrap_or_default();
            let described_by = described.clone();
            view! {
                cx =>
                <form
                    method="post"
                    action=(url)
                    class="contents"
                    data-mutation-submit=""
                >
                    (crate::csrf::field(cx, &token))
                    button(
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        attrs: attributes! { type="submit" aria-describedby=(described_by) },
                        (label)
                    )
                </form>
            }
            .boxed()
        })
        .collect();
    view! {
        cx =>
        table_cell(
            attrs: attributes! { style=(actions_min.as_deref()) },
            <div class="flex items-center justify-end gap-1">
                for form in custom {
                    (form)
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
                if let Some((url, action, dialog)) = delete {
                    <a
                        href=(url)
                        data-row-delete-trigger=(dialog)
                        data-row-delete-action=(action)
                        class=(delete_class)
                        aria-label="Delete"
                        aria-describedby=(described.clone())
                        title="Delete"
                    >
                        icon(
                            data: tablo_ui::icons::TRASH,
                            attrs: attributes! { class="text-destructive" }
                        )
                    </a>
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
    delete_url: Option<String>,
    /// The row's delete POST target handed to the shared dialog.
    delete_action: Option<String>,
    /// The custom row actions this record allows: each button's label,
    /// its POST target, and whether it asks first through the dialog.
    custom: Vec<(String, String, bool)>,
    /// Whether the row renders a bulk checkbox.
    selectable: bool,
    /// The row's group label, when `?group_by=` names the declared group.
    group: Option<String>,
    /// The header this row renders above itself, `Some` only on the first row
    /// of its group.
    group_header: Option<GroupHeader>,
}

/// One page-local group header: the label with its page-local count and the stable DOM id the
/// injected header row carries.
#[derive(Clone)]
struct GroupHeader {
    /// `"{label} ({n} on this page)"`.
    text: String,
    dom_id: String,
}

impl<M> Table<M> {
    /// Project the loaded page into the row presentation the template renders.
    pub(super) fn row_views<'a>(
        &self,
        cx: &'a Cx,
        state: &TableState,
        path: &str,
        page: &TablePage<M>,
        group_key: Option<&GroupKey<M>>,
    ) -> Vec<RowView<'a>>
    where
        M: toasty::schema::Model,
    {
        let delete_url_base = self
            .delete_prefix
            .as_ref()
            .map(|_| state.row_url_base(path));
        let gated = self.delete_prefix.is_some()
            || self.edit_prefix.is_some()
            || self.view_prefix.is_some();
        let bulk_delete = self.bulk_delete_enabled();
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
                let edit_url = self
                    .edit_prefix
                    .as_ref()
                    .filter(|_| actions.edit)
                    .map(|prefix| self.action_url(row_edit_url(prefix, &key)));
                let view_url = self
                    .view_prefix
                    .as_ref()
                    .filter(|_| actions.view)
                    .map(|prefix| row_view_url(prefix, &key));
                let delete_url = delete_url_base
                    .as_ref()
                    .filter(|_| actions.delete)
                    .map(|base| base.delete_dialog(&key));
                let delete_action = self
                    .delete_prefix
                    .as_ref()
                    .filter(|_| actions.delete)
                    .map(|prefix| self.action_url(delete_action_url(prefix, &key)));
                let custom = self
                    .actions_prefix
                    .as_ref()
                    .map(|prefix| {
                        self.row_custom_actions()
                            .filter(|action| (action.allowed)(row))
                            .map(|action| {
                                let url = row_action_url(prefix, &key, action.name);
                                (action.label.clone(), self.action_url(url), action.confirm)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let selectable = (bulk_delete && actions.delete)
                    || self
                        .bulk_custom_actions()
                        .any(|action| (action.allowed)(row));
                RowView {
                    key,
                    cells,
                    view_url,
                    edit_url,
                    delete_url,
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
