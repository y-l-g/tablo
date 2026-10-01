//! Row projection: the page's rows as the owned views the template renders.

use std::borrow::Cow;

use tablo_ui::{ButtonSize, ButtonVariant, button, button_variants, table_cell, table_row};
use topcoat::{context::Cx, icon::icon, view::*};

use super::super::{
    super::{
        page::TablePage,
        state::{
            TableState, delete_action_url, group_header_dom_id, row_action_url, row_dom_id,
            row_edit_url, row_view_url,
        },
    },
    GroupKey, RowActions, Table,
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
}

/// One rendered row, keyed for the body's diff.
pub(super) struct RenderedRow<'a> {
    /// The row's display key: stable for the record, never a loop index.
    pub(super) key: String,
    /// The group header the row opens, if any, then the row itself.
    pub(super) view: BoxView<'a>,
}

/// Render the page's rows for the table body. One body serves grouped and
/// ungrouped pages: a grouped row carries the header its group's first row
/// owns, so the header lands immediately above its own rows.
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
    let bulk_cell = chrome.with_bulk.then(|| {
        if row.selectable {
            let value = row.record_id.clone();
            view! {
                cx =>
                table_cell(
                    <input
                        type="checkbox"
                        value=(value)
                        aria-label="Select row"
                        data-row-select=""
                    >
                )
            }
            .boxed()
        } else {
            // A refused row renders no checkbox: selecting it could only
            // produce a batch the handler refuses. The cell stays so the row
            // keeps its shape.
            view! { cx => table_cell() }.boxed()
        }
    });
    // `row.cells` is built column-for-column, so the zip pairs each cell with
    // the column that owns its width. The cell repeats the width its header
    // declares and truncates: under the table's fixed layout a value wider than
    // the column clips to an ellipsis instead of stretching the column.
    let cells: Vec<(BoxView<'a>, Option<Cow<'static, str>>)> = std::mem::take(&mut row.cells)
        .into_iter()
        .zip(chrome.cell_widths.iter().cloned())
        .collect();
    // Every row carries the actions cell its header declares; a row refused
    // every link keeps an empty cell so the row keeps its shape.
    let actions = chrome
        .with_actions
        .then(|| render_actions(cx, &row, chrome));
    let dom_id = row_dom_id(&row.key);
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
    // Icon-only controls: the label rides `aria-label` for assistive tech and
    // `title` for a pointer, and the icon carries it on screen.
    let link_class = button_variants(ButtonVariant::Ghost, ButtonSize::Icon);
    let edit_class = link_class.clone();
    let delete_class = link_class.clone();
    // Each custom action is its own POST form: a plain submit and a 303, so
    // it needs no script. Its button carries the label, not an icon.
    let csrf = (!row.custom.is_empty()).then(|| crate::csrf::current_token(cx));
    let custom: Vec<BoxView<'a>> = row
        .custom
        .iter()
        .map(|(label, url)| {
            let label = label.clone();
            let url = url.clone();
            let token = csrf.clone().unwrap_or_default();
            view! {
                cx =>
                <form method="post" action=(url) class="contents" data-row-action="">
                    (crate::csrf::field(cx, &token))
                    button(
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        attrs: attributes! { type="submit" },
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
                        (crate::resource::runtime_link(cx, &url))
                        class=(link_class)
                        aria-label="View"
                        title="View"
                    >
                        icon(data: tablo_ui::icons::EYE)
                    </a>
                }
                if let Some(url) = edit_url {
                    <a
                        (crate::resource::runtime_link(cx, &url))
                        class=(edit_class)
                        aria-label="Edit"
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
                        title="Delete"
                    >
                        // The glyph carries the destructive color: the ghost
                        // variant already sets the control's text color.
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

/// Precomputed per-row presentation for the table body: the display row key,
/// the record key, the rendered cells, and the optional Edit / delete-dialog
/// action URLs. A struct (not a tuple): five anonymous positions would
/// mislead readers and trip `clippy::type_complexity`.
///
/// `key` is the display projection (keyed diffs, DOM ids);
/// `record_id` is the record projection (URLs, bulk values), resolved
/// by handlers as the typed PK.
pub(super) struct RowView<'a> {
    key: String,
    record_id: String,
    /// One rendered cell per column, in column order.
    cells: Vec<BoxView<'a>>,
    view_url: Option<String>,
    edit_url: Option<String>,
    delete_url: Option<String>,
    /// The row's delete POST target (`{prefix}/{key}/delete`): the
    /// Delete control hands it to the shared dialog before opening it, so the
    /// confirmed POST keeps the route the `?delete=` fallback uses.
    delete_action: Option<String>,
    /// The custom row actions this record allows: each button's label and
    /// its POST target.
    custom: Vec<(String, String)>,
    /// Whether the row renders a bulk checkbox: a row that neither bulk
    /// delete nor any bulk custom action allows renders none, so `bulk.js`
    /// never sees its key.
    selectable: bool,
    /// The row's group label, when `?group_by=` named the declared group.
    /// Carried on every row so the page-local shim can order by it.
    group: Option<String>,
    /// The header this row renders above itself, `Some` only on the first row
    /// of its group.
    group_header: Option<GroupHeader>,
}

/// One page-local group header: the label with its page-local count,
/// and the stable DOM id the injected header row carries so the in-place morph
/// can follow it (`row_dom_id`'s contract).
#[derive(Clone)]
struct GroupHeader {
    /// `"{label} ({n} on this page)"`.
    text: String,
    /// [`group_header_dom_id`] of the label.
    dom_id: String,
}

impl<M> Table<M> {
    /// Project the loaded page into the row presentation the template renders.
    ///
    /// Precomputed so template bodies capture only owned data — the lazy view
    /// outlives the render call, so it must never borrow `self` or `page`.
    ///
    /// The per-row delete URL opens the confirmation dialog on the list page
    /// (`?delete=<key>`); the per-row edit URL links to `{prefix}/{key}/edit`.
    /// Both — and the bulk checkbox values — carry the *record* key, resolved by
    /// handlers as the model's typed PK; the display `key` stays on keyed diffs
    /// and DOM ids.
    ///
    /// The chrome prefixes say which links the table *can* render; the
    /// [`Table::row_actions`] policy says which of them *this* record may use.
    /// A denied action emits no URL, and a row denied `delete` renders no bulk
    /// checkbox. The policy is consulted only when a prefix is wired.
    ///
    /// The delete URL's shared parameters are encoded once for the whole page:
    /// rebuilding them per row is work a client can inflate with an oversized
    /// query.
    ///
    /// Page-local grouping is display-only: `group_by` is a bare key closure with
    /// no lens, so no `ORDER BY` is derivable and a group cannot span pages. The
    /// shim therefore reorders *this page's* rows by the group label — a stable
    /// sort, so rows keep the query's order inside their group — and hangs each
    /// group's header off its first row. The query, its cursors and the export
    /// keep the declared ordering.
    ///
    /// Row keys must be injective within a page: duplicates corrupt keyed diffs
    /// and bulk selection.
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
                let key = (self.row_key)(row);
                let record_id = (self.record_key)(row);
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
                    .map(|prefix| self.action_url(row_edit_url(prefix, &record_id)));
                let view_url = self
                    .view_prefix
                    .as_ref()
                    .filter(|_| actions.view)
                    .map(|prefix| row_view_url(prefix, &record_id));
                let delete_url = delete_url_base
                    .as_ref()
                    .filter(|_| actions.delete)
                    .map(|base| base.delete_dialog(&record_id));
                // The shared dialog's POST target for this row: the
                // row control hands it over before opening the dialog, so the
                // action and the control come from the one policy decision.
                let delete_action = self
                    .delete_prefix
                    .as_ref()
                    .filter(|_| actions.delete)
                    .map(|prefix| self.action_url(delete_action_url(prefix, &record_id)));
                let custom = self
                    .actions_prefix
                    .as_ref()
                    .map(|prefix| {
                        self.row_custom_actions()
                            .filter(|action| (action.allowed)(row))
                            .map(|action| {
                                let url = row_action_url(prefix, &record_id, action.name);
                                (action.label.clone(), self.action_url(url))
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
                    record_id,
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
                // The count is page-local, and says so: a group split across
                // pages must not read as a table total. The header
                // carries an id derived from its label — never from its
                // position — so the in-place morph can follow it.
                row_data[start].group_header = Some(GroupHeader {
                    dom_id: group_header_dom_id(&label),
                    text: format!("{label} ({} on this page)", end - start),
                });
                start = end;
            }
        }
        debug_assert!(
            {
                let mut seen = std::collections::HashSet::new();
                row_data.iter().all(|row| seen.insert(row.key.clone()))
            },
            "duplicate table keys in one page: the key projection must be injective"
        );
        row_data
    }
}
