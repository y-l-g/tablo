//! Semantic queries over rendered HTML for tests.
//!
//! The table and form renderers own their markup; tests assert the behavior
//! the markup carries. Each helper reads one stable hook:
//! `tr[id^="row-"]` for rows, `[aria-label]` links and buttons for actions,
//! `#{name}-error` for field errors, `[data-filter-name]` for filters, `[data-empty]` for the
//! zero-rows message.
//!
//! The queries assume one table per document and flat renderer markup:
//! rows never nest, and cells carry text rather than nested tables.

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

/// One `<option>` of a filter select.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterOption {
    /// The submitted value.
    pub value: String,
    /// The shown label.
    pub label: String,
    /// Whether the request selected it.
    pub selected: bool,
}

/// Why a table rendered no rows, and the links it offers out of that state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmptyTable {
    /// What emptied it: `"none"` for no records, `"search"` or `"filters"` when the request
    /// narrowed it.
    pub reason: String,
    /// The link that clears the search or the filters, `None` when nothing narrowed the table.
    pub clear: Option<String>,
    /// The link back to the first page, `None` without a cursor.
    pub first_page: Option<String>,
}

/// The zero-rows message of `html`, or `None` when the table renders rows.
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
    Some(EmptyTable {
        reason: attr_value(&attrs, "data-empty").unwrap_or_default(),
        clear: link("clear"),
        first_page: link("first-page"),
    })
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

/// The validation message rendered for `name`, or `None` when the field
/// renders no error.
pub fn field_error(html: &str, name: &str) -> Option<String> {
    let target = format!("{name}-error");
    let (_, _, inner) = tags_where(html, |_, attrs| {
        attr_value(attrs, "id").as_deref() == Some(target.as_str())
    })
    .into_iter()
    .next()?;
    let text = strip_text(&inner);
    (!text.is_empty()).then_some(text)
}

/// The options of the select filter `name`, or `None` when `name` has no
/// select control.
pub fn filter_options(html: &str, name: &str) -> Option<Vec<FilterOption>> {
    for (tag, _, inner) in tags_where(html, |_, attrs| {
        attr_value(attrs, "data-filter-name").as_deref() == Some(name)
    }) {
        if tag != "select" {
            return None;
        }
        let mut options = Vec::new();
        for (_, option_attrs, option_inner) in elements(&inner, "option") {
            options.push(FilterOption {
                value: attr_value(&option_attrs, "value").unwrap_or_default(),
                label: strip_text(&option_inner),
                selected: is_selected(&option_attrs),
            });
        }
        return Some(options);
    }
    None
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
    let after = &html[open_end + 1..];
    after
        .find(format!("</{tag}>").as_str())
        .map(|close_at| after[..close_at].to_string())
        .unwrap_or_default()
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
