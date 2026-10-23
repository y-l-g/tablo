//! Row projection: the page's rows as the owned views the template renders.

use tablo_ui::{table_cell, table_row};
use topcoat::{context::Cx, runtime::Event, view::*};

use super::{
    super::{RowActions, TableSignals, WiredTable},
    CHROME_CELL_CLASS, Frame,
};
use crate::{
    panel::url::{delete_action_url, row_action_url, row_edit_url, row_view_url},
    t,
    table::{GroupKey, TablePage, bulk_token, group_header_dom_id, row_dom_id},
};

mod actions;

/// What every row of one render shares: the chrome columns, each column's cell layout, and the
/// table's signals and write form.
pub(super) struct RowChrome {
    pub(super) with_bulk: bool,
    pub(super) with_actions: bool,
    /// Columns a group header spans: the data columns plus the chrome.
    pub(super) header_colspan: usize,
    /// Each data column's cell layout, in column order.
    pub(super) cells: Vec<CellLayout>,
    pub(super) signals: TableSignals,
    /// The DOM id of the table's write form.
    pub(super) form: String,
    /// Whether the row's actions sit in a menu.
    pub(super) action_menu: bool,
    /// The table's name in its rows' DOM ids, distinct per table on the page: two tables can
    /// hold one key, and a duplicate id would point a row's menu or description at the other
    /// table.
    pub(super) id_scope: String,
}

impl RowChrome {
    /// The DOM id of the row holding `key`.
    fn row_id(&self, key: &str) -> String {
        row_dom_id(&self.id_scope, key)
    }
}

/// How one column's body cells take their width and align.
#[derive(Clone)]
pub(super) struct CellLayout {
    /// Its alignment's class, if any.
    pub(super) class: Option<&'static str>,
    /// Its width's `style`, for an exact width.
    pub(super) style: Option<String>,
    /// The classes of the box its content sits in, which caps or wraps it.
    pub(super) content: Option<&'static str>,
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
        let id = group_header_dom_id(&chrome.id_scope, &header.label);
        let label = header.label.clone();
        view! {
            cx =>
            table_row(
                attrs: attributes! { id=(id) data-group=(label) },
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
    let dom_id = chrome.row_id(&row.key);
    let bulk_cell = chrome.with_bulk.then(|| {
        if row.selectable {
            let described = dom_id.clone();
            let bulk = chrome.signals.bulk.clone();
            let token = bulk_token(&row.key);
            let value = row.key.clone();
            view! {
                cx =>
                table_cell(
                    attrs: attributes! { class=(CHROME_CELL_CLASS) },
                    <input
                        type="checkbox"
                        value=(value)
                        aria-label=(t!(cx, "tablo-list-select-row"))
                        data-row-select=""
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
            view! { cx => table_cell(attrs: attributes! { class=(CHROME_CELL_CLASS) }) }.boxed()
        }
    });
    // The whole row opens its record: an empty link in the first cell stretches over the row, and
    // the cells' own links and controls rise above it.
    let record_url = row.view_url.clone().or_else(|| row.edit_url.clone());
    let row_class = record_url.is_some().then_some(RECORD_ROW_CLASS);
    let cells: Vec<(BoxView<'a>, CellLayout)> = std::mem::take(&mut row.cells)
        .into_iter()
        .zip(chrome.cells.iter().cloned())
        .enumerate()
        .map(|(index, (cell, layout))| match record_url.as_deref() {
            Some(url) if index == 0 => (with_record_link(cx, url, cell), layout),
            _ => (cell, layout),
        })
        .collect();
    let actions = chrome
        .with_actions
        .then(|| actions::render_actions(cx, &row, chrome));
    view! {
        cx =>
        if let Some(header) = header {
            (header)
        }
        table_row(
            attrs: attributes! { id=(dom_id) class=(row_class) },
            if let Some(cell) = bulk_cell {
                (cell)
            }
            for (cell, layout) in cells {
                table_cell(
                    attrs: attributes! { class=(layout.class) style=(layout.style) },
                    if let Some(content) = layout.content {
                        <div class=(content)>(cell)</div>
                    } else {
                        (cell)
                    }
                )
            }
            if let Some(actions) = actions {
                (actions)
            }
        )
    }
    .boxed()
}

/// A row that opens its record: it positions the record link, and lifts every link and control
/// in its cells above that link, so they keep their own clicks.
const RECORD_ROW_CLASS: &str = "relative [&_:is(a,button,input,select,textarea,label,summary):not([data-record-link])]:relative [&_:is(a,button,input,select,textarea,label,summary):not([data-record-link])]:z-10";

/// `cell` after an empty link to the row's record that stretches over the whole row. The link
/// wraps no cell, so a cell's own link never nests in it. The View or Edit control is the
/// keyboard's and the screen reader's way to the record, so the link takes no tab stop and stays
/// out of the tree.
fn with_record_link<'a>(cx: &'a Cx, url: &str, cell: BoxView<'a>) -> BoxView<'a> {
    let link = crate::navigation::runtime_link(cx, url);
    view! {
        cx =>
        <a
            (link)
            class="absolute inset-0"
            tabindex="-1"
            aria-hidden="true"
            data-record-link=""
        ></a>
        (cell)
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

/// One page-local group header: the label with its page-local count, and the label its injected
/// row's stable DOM id derives from.
#[derive(Clone)]
struct GroupHeader {
    /// `"{label} ({n} on this page)"`.
    text: String,
    /// The group's label, which its DOM id derives from.
    label: String,
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
                let cells: Vec<BoxView<'a>> = self
                    .visible_columns()
                    .map(|col| col.cell(cx, row))
                    .collect();
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
                    text: format!("{label} ({} on this page)", end - start),
                    label,
                });
                start = end;
            }
        }
        row_data
    }
}
