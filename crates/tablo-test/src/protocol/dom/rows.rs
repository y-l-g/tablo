//! The rows of a rendered table: their cells, keys, actions, links and tint.

use tablo_core::dom::row;

use super::{Attrs, attr_value, elements, open_tags, strip_text};
use crate::protocol::controls::{DisabledAction, disabled_actions};

/// One data row of a rendered table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The bulk checkbox value, the record key when the row is selectable.
    pub select_value: Option<String>,
    /// The cell texts in column order, including the bulk and actions cells.
    pub cells: Vec<String>,
    /// The row's actions, nested so hrefs have one source.
    pub actions: RowActions,
    /// The targets of its cells' links, in document order.
    pub links: Vec<String>,
    /// The values its cells' copy buttons copy, in document order.
    pub copies: Vec<String>,
    /// The color the table's row color tints it in, by its `#[option(color = "…")]` name.
    pub color: Option<String>,
}

/// The action targets of one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowActions {
    /// The `View` link target, `None` when policy hides it.
    pub view: Option<String>,
    /// The `Edit` link target, `None` when policy hides it.
    pub edit: Option<String>,
    /// The `Delete` POST target, carried as its button's `formaction`; `None` when policy hides
    /// it.
    pub delete_action: Option<String>,
    /// The custom actions the row shows disabled, in document order.
    pub disabled: Vec<DisabledAction>,
}

/// The data rows of `html` in document order, excluding group headers.
pub fn rows(html: &str) -> Vec<Row> {
    let mut out = Vec::new();
    for (_, attrs, inner) in elements(html, "tr") {
        let Some(id) = attr_value(&attrs, "id") else {
            continue;
        };
        if !id.starts_with("row-") {
            continue;
        }
        out.push(row(&attrs, &inner));
    }
    out
}

/// One group of a grouped table: its header's label and the rows under it on this page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The label the group's header shows.
    pub label: String,
    /// The group's rows on this page, in document order.
    pub rows: Vec<Row>,
}

/// The groups of `html` in document order, empty when the table renders ungrouped. A group holds
/// the rows from its header to the next one.
pub fn groups(html: &str) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    for (_, attrs, inner) in elements(html, "tr") {
        if let Some(label) = attr_value(&attrs, "data-group") {
            out.push(Group {
                label,
                rows: Vec::new(),
            });
        } else if attr_value(&attrs, "id").is_some_and(|id| id.starts_with("row-"))
            && let Some(group) = out.last_mut()
        {
            group.rows.push(row(&attrs, &inner));
        }
    }
    out
}

/// The record key of every row in document order, read from its bulk checkbox: a row with no
/// checkbox has none to read.
pub fn row_keys(html: &str) -> Vec<String> {
    rows(html)
        .into_iter()
        .filter_map(|row| row.select_value)
        .collect()
}

/// The action targets of the row whose checkbox value or action URL carries
/// `key`, or `None` when no row matches.
pub fn row_actions(html: &str, key: &str) -> Option<RowActions> {
    rows(html).into_iter().find_map(|row| {
        let matches = row.select_value.as_deref() == Some(key)
            || row
                .actions
                .view
                .as_deref()
                .is_some_and(|href| href_contains_key(href, key))
            || row
                .actions
                .edit
                .as_deref()
                .is_some_and(|href| href_contains_key(href, key))
            || row
                .actions
                .delete_action
                .as_deref()
                .is_some_and(|action| href_contains_key(action, key));
        matches.then_some(row.actions)
    })
}

fn row(row_attrs: &Attrs, row_html: &str) -> Row {
    let mut select_value = None;
    for (tag, attrs, _) in open_tags(row_html) {
        if tag == "input" && attr_value(&attrs, "data-row-select").is_some() {
            select_value = attr_value(&attrs, "value");
            break;
        }
    }
    let mut cells = Vec::new();
    for (_, _, inner) in elements(row_html, "td") {
        cells.push(strip_text(&inner));
    }
    let mut view_href = None;
    let mut edit_href = None;
    let mut delete_action = None;
    let mut links = Vec::new();
    let mut copies = Vec::new();
    for (_, attrs, _) in open_tags(row_html) {
        if attr_value(&attrs, "data-cell-link").is_some() {
            links.extend(attr_value(&attrs, "href"));
        }
        if let Some(value) = attr_value(&attrs, "data-copy") {
            copies.push(value);
        }
        match attr_value(&attrs, "data-row-action").as_deref() {
            Some(row::VIEW) => view_href = attr_value(&attrs, "href"),
            Some(row::EDIT) => edit_href = attr_value(&attrs, "href"),
            Some(row::DELETE) => delete_action = attr_value(&attrs, "formaction"),
            _ => {}
        }
    }
    Row {
        select_value,
        cells,
        actions: RowActions {
            view: view_href,
            edit: edit_href,
            delete_action,
            disabled: disabled_actions(row_html),
        },
        links,
        copies,
        color: attr_value(row_attrs, "data-row-color"),
    }
}

fn href_contains_key(href: &str, key: &str) -> bool {
    href.split(['?', '#', '/', '&'])
        .any(|segment| segment == key)
}
