//! Semantic queries over rendered HTML for tests.
//!
//! The table and form renderers own their markup; tests assert the behavior
//! the markup carries. Each helper reads one stable hook:
//! `tr[id^="row-"]` for rows, `tr[data-group]` for group headers, `[aria-label]` links and buttons
//! for actions, `#{name}-error` and its `data-error` for field errors, `[data-filter-name]` for
//! filters, `[data-empty]` for the zero-rows message, `[data-sidebar=menu-item]` for sidebar
//! entries, `th[data-column]` for the shown columns, and the `column` checkboxes for the column
//! picker.
//!
//! The queries assume one table per document: rows never nest, and cells carry text rather than
//! nested tables.

use tablo_core::dom::{empty, error};

type Attrs = Vec<(String, Option<String>)>;
type Element = (String, Attrs, String);

/// One data row of a rendered table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The bulk checkbox value, the record key when the row is selectable.
    pub select_value: Option<String>,
    /// The cell texts in column order, including the bulk and actions cells.
    pub cells: Vec<String>,
    /// The row's actions, nested so hrefs have one source.
    pub actions: RowActions,
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
}

/// One `<option>` of a filter select, or one checkbox of a filter checklist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterOption {
    /// The submitted value.
    pub value: String,
    /// The shown label.
    pub label: String,
    /// Whether the request selected it, or checked it.
    pub selected: bool,
}

/// One toggleable column the column picker offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerColumn {
    /// The column's name, which the picker posts.
    pub name: String,
    /// Whether the reader's choice shows it: its box is checked.
    pub shown: bool,
}

/// Why a table rendered no rows, and the links it offers out of that state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyTable {
    /// What emptied it.
    pub reason: EmptyReason,
    /// The link that clears the search or the filters, `None` when nothing narrowed the table.
    pub clear: Option<String>,
    /// The link back to the first page, `None` without a cursor.
    pub first_page: Option<String>,
}

/// What emptied a table, as its zero-rows message's `data-empty` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EmptyReason {
    /// The table holds no records.
    NoRecords,
    /// The request's search matched no record.
    Search,
    /// The request's filters matched no record.
    Filters,
}

/// The zero-rows message of `html`, or `None` when the table renders rows.
///
/// # Panics
///
/// When the message names no reason this version knows: a renderer that drops `data-empty`'s
/// value would otherwise read as any reason.
pub fn empty_table(html: &str) -> Option<EmptyTable> {
    let (_, attrs, inner) = tags_where(html, |_, attrs| attr_value(attrs, "data-empty").is_some())
        .into_iter()
        .next()?;
    let link = |kind: &str| {
        open_tags(&inner)
            .into_iter()
            .find(|(_, attrs, _)| attr_value(attrs, "data-empty-link").as_deref() == Some(kind))
            .and_then(|(_, attrs, _)| attr_value(&attrs, "href"))
    };
    let reason = match attr_value(&attrs, "data-empty").as_deref() {
        Some(empty::NONE) => EmptyReason::NoRecords,
        Some(empty::SEARCH) => EmptyReason::Search,
        Some(empty::FILTERS) => EmptyReason::Filters,
        other => panic!("the zero-rows message names no known reason: {other:?}"),
    };
    Some(EmptyTable {
        reason,
        clear: link(empty::CLEAR_LINK),
        first_page: link(empty::FIRST_PAGE_LINK),
    })
}

/// The names of the columns the table's header shows, in order, without the bulk and actions
/// columns.
pub fn columns(html: &str) -> Vec<String> {
    open_tags(html)
        .into_iter()
        .filter(|(tag, _, _)| tag == "th")
        .filter_map(|(_, attrs, _)| attr_value(&attrs, "data-column"))
        .collect()
}

/// The toggleable columns the column picker offers, in order, or `None` when the table renders
/// no picker.
pub fn column_picker(html: &str) -> Option<Vec<PickerColumn>> {
    let columns: Vec<PickerColumn> = open_tags(html)
        .into_iter()
        .filter(|(tag, attrs, _)| {
            tag == "input"
                && attr_value(attrs, "type").as_deref() == Some("checkbox")
                && attr_value(attrs, "name").as_deref() == Some("column")
        })
        .map(|(_, attrs, _)| PickerColumn {
            name: attr_value(&attrs, "value").unwrap_or_default(),
            shown: attrs.iter().any(|(name, _)| name == "checked"),
        })
        .collect();
    (!columns.is_empty()).then_some(columns)
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
        out.push(row(&inner));
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
            group.rows.push(row(&inner));
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

/// The first `href` in `html` that contains `needle`, its entities decoded.
pub fn find_href_with(html: &str, needle: &str) -> Option<String> {
    open_tags(html)
        .into_iter()
        .filter_map(|(_, attrs, _)| attr_value(&attrs, "href"))
        .find(|href| href.contains(needle))
}

/// Whether the button posting to `action` asks first: a confirming trigger is a plain button that
/// opens the dialog, a direct write is a submit button. `None` when no button posts there.
pub fn confirms_first(html: &str, action: &str) -> Option<bool> {
    open_tags(html)
        .into_iter()
        .find(|(tag, attrs, _)| {
            tag == "button" && attr_value(attrs, "formaction").as_deref() == Some(action)
        })
        .map(|(_, attrs, _)| attr_value(&attrs, "type").as_deref() == Some("button"))
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

/// Why a field renders an error, as its error slot's `data-error` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Posted empty, and the field has no blank answer.
    Required,
    /// Posted a value the field, the resource's validation or the uploader refused.
    Invalid,
    /// A choice whose options the user may not read.
    Unavailable,
}

/// Why the field posting `name` was refused, or `None` when it renders no error.
///
/// # Panics
///
/// When the slot names no kind this version knows: a renderer that drops `data-error` would
/// otherwise read as any refusal.
///
/// Assert on the kind rather than the sentence, which is wording; [`field_error_message`] reads
/// the sentence for a test whose subject is an app's own message.
pub fn field_error(html: &str, name: &str) -> Option<ErrorKind> {
    let (attrs, _) = error_slot(html, name)?;
    Some(match attr_value(&attrs, "data-error").as_deref() {
        Some(error::REQUIRED) => ErrorKind::Required,
        Some(error::INVALID) => ErrorKind::Invalid,
        Some(error::UNAVAILABLE) => ErrorKind::Unavailable,
        other => panic!("the error slot of `{name}` names no known kind: {other:?}"),
    })
}

/// The sentence rendered under the field posting `name`, or `None` when it renders no error.
pub fn field_error_message(html: &str, name: &str) -> Option<String> {
    error_slot(html, name).map(|(_, text)| text)
}

/// The attributes and text of the error slot `#{name}-error`, when it renders one.
fn error_slot(html: &str, name: &str) -> Option<(Attrs, String)> {
    let target = format!("{name}-error");
    let (_, attrs, inner) = tags_where(html, |_, attrs| {
        attr_value(attrs, "id").as_deref() == Some(target.as_str())
    })
    .into_iter()
    .next()?;
    let text = strip_text(&inner);
    (!text.is_empty()).then_some((attrs, text))
}

/// The options of the select or checklist filter `name`, or `None` when `name` names neither. A
/// checklist offering no option reads as an empty list, since its control still renders.
pub fn filter_options(html: &str, name: &str) -> Option<Vec<FilterOption>> {
    let (tag, _, inner) = tags_where(html, |_, attrs| {
        attr_value(attrs, "data-filter-name").as_deref() == Some(name)
    })
    .into_iter()
    .next()?;
    if tag != "select" {
        let options: Vec<FilterOption> = elements(&inner, "label")
            .into_iter()
            .filter_map(|(_, _, inner)| {
                let (_, attrs, _) = open_tags(&inner).into_iter().find(|(tag, attrs, _)| {
                    tag == "input" && attr_value(attrs, "type").as_deref() == Some("checkbox")
                })?;
                Some(FilterOption {
                    value: attr_value(&attrs, "value").unwrap_or_default(),
                    label: strip_text(&inner),
                    selected: attrs.iter().any(|(name, _)| name == "checked"),
                })
            })
            .collect();
        // A checklist renders the button that opens it, with or without an option; another
        // control's container, such as a date filter's, renders none.
        return inner.contains("<button").then_some(options);
    }
    let options = elements(&inner, "option")
        .into_iter()
        .map(|(_, attrs, inner)| FilterOption {
            value: attr_value(&attrs, "value").unwrap_or_default(),
            label: strip_text(&inner),
            selected: is_selected(&attrs),
        })
        .collect();
    Some(options)
}

/// The badge text of the sidebar entry linking to `href`, or `None` when that entry shows no
/// badge or no entry links there.
pub fn nav_badge(html: &str, href: &str) -> Option<String> {
    let menu = |name: &'static str| {
        move |_: &str, attrs: &Attrs| attr_value(attrs, "data-sidebar").as_deref() == Some(name)
    };
    let (_, _, entry) = tags_where(html, menu("menu-item"))
        .into_iter()
        .find(|(_, _, inner)| {
            open_tags(inner).iter().any(|(tag, attrs, _)| {
                tag == "a" && attr_value(attrs, "href").as_deref() == Some(href)
            })
        })?;
    let (_, _, badge) = tags_where(&entry, menu("menu-badge")).into_iter().next()?;
    Some(strip_text(&badge))
}

fn row(row_html: &str) -> Row {
    let mut select_value = None;
    for (tag, attrs, _) in open_tags(row_html) {
        if tag == "input" && attr_value(&attrs, "aria-label").as_deref() == Some("Select row") {
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
    for (tag, attrs, _) in open_tags(row_html) {
        match (tag.as_str(), attr_value(&attrs, "aria-label").as_deref()) {
            ("a", Some("View")) => view_href = attr_value(&attrs, "href"),
            ("a", Some("Edit")) => edit_href = attr_value(&attrs, "href"),
            ("button", Some("Delete")) => delete_action = attr_value(&attrs, "formaction"),
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
        },
    }
}

fn href_contains_key(href: &str, key: &str) -> bool {
    href.split(['?', '#', '/', '&'])
        .any(|segment| segment == key)
}

fn is_selected(attrs: &Attrs) -> bool {
    attrs.iter().any(|(name, _)| name == "selected")
}

fn attr_value(attrs: &Attrs, name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(attr, _)| attr == name)
        .and_then(|(_, value)| value.clone())
}

/// The tags `keep` accepts, each with its inner HTML up to the first matching close tag.
///
/// Only an accepted tag's inner HTML is read, so a query over one element costs one scan of the
/// document rather than one per tag.
fn tags_where(html: &str, keep: impl Fn(&str, &Attrs) -> bool) -> Vec<Element> {
    open_tags(html)
        .into_iter()
        .filter(|(tag, attrs, _)| keep(tag, attrs))
        .map(|(tag, attrs, open_end)| {
            let inner = inner_html(html, &tag, open_end);
            (tag, attrs, inner)
        })
        .collect()
}

fn inner_html(html: &str, tag: &str, open_end: usize) -> String {
    if is_void(tag) {
        return String::new();
    }
    // Same-name elements nest (a `div` in a `div`), so the close is the one that returns the
    // depth to zero.
    let after = &html[open_end + 1..];
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut depth = 1usize;
    let mut pos = 0;
    while let Some(at) = after[pos..].find('<') {
        let at = pos + at;
        let rest = &after[at..];
        if rest.starts_with(close.as_str()) {
            depth -= 1;
            if depth == 0 {
                return after[..at].to_string();
            }
            pos = at + close.len();
        } else if rest.starts_with(open.as_str())
            && rest[open.len()..].starts_with([' ', '>', '/', '\t', '\n'])
        {
            depth += 1;
            pos = at + open.len();
        } else {
            pos = at + 1;
        }
    }
    String::new()
}

fn elements(html: &str, tag: &str) -> Vec<Element> {
    tags_where(html, |name, _| name == tag)
}

fn open_tags(html: &str) -> Vec<(String, Attrs, usize)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(start) = html[pos..].find('<') {
        let abs_start = pos + start;
        let Some(end) = tag_end(html, abs_start) else {
            break;
        };
        let raw = &html[abs_start + 1..end];
        pos = end + 1;
        if raw.starts_with('!') || raw.starts_with('?') || raw.starts_with('/') {
            continue;
        }
        let (tag, attrs) = parse_tag(raw);
        if tag.is_empty() {
            continue;
        }
        out.push((tag, attrs, end));
    }
    out
}

fn parse_tag(raw: &str) -> (String, Attrs) {
    let mut attrs = Vec::new();
    let bytes = raw.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() && bytes[pos] != b'/' {
        pos += 1;
    }
    let tag = raw[..pos].to_ascii_lowercase();
    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() || bytes[pos] == b'/' {
            break;
        }
        let name_start = pos;
        while pos < bytes.len()
            && (bytes[pos].is_ascii_alphanumeric()
                || matches!(bytes[pos], b'-' | b'_' | b':' | b'.' | b'@'))
        {
            pos += 1;
        }
        if name_start == pos {
            pos += 1;
            while pos < bytes.len() && !raw.is_char_boundary(pos) {
                pos += 1;
            }
            continue;
        }
        let name = raw[name_start..pos].to_ascii_lowercase();
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() || bytes[pos] != b'=' {
            attrs.push((name, None));
            continue;
        }
        pos += 1;
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() {
            attrs.push((name, Some(String::new())));
            break;
        }
        let quote = bytes[pos];
        if quote == b'"' || quote == b'\'' {
            pos += 1;
            let value_start = pos;
            while pos < bytes.len() && bytes[pos] != quote {
                pos += 1;
            }
            attrs.push((
                name,
                Some(decode_entities(&raw[value_start..pos.min(bytes.len())])),
            ));
            pos = (pos + 1).min(bytes.len());
        } else {
            let value_start = pos;
            while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            attrs.push((name, Some(decode_entities(&raw[value_start..pos]))));
        }
    }
    (tag, attrs)
}

fn tag_end(html: &str, start: usize) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut quote = None;
    let mut pos = start + 1;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if let Some(q) = quote {
            if byte == q {
                quote = None;
            }
        } else if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if byte == b'>' {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

fn is_void(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn strip_text(fragment: &str) -> String {
    let mut text = String::new();
    let mut pos = 0;
    while pos < fragment.len() {
        if let Some(start) = fragment[pos..].find('<') {
            text.push_str(&fragment[pos..pos + start]);
            let abs = pos + start;
            match tag_end(fragment, abs) {
                Some(end) => pos = end + 1,
                None => break,
            }
        } else {
            text.push_str(&fragment[pos..]);
            break;
        }
    }
    decode_entities(&text).trim().to_string()
}

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests;
