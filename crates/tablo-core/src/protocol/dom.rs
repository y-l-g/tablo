//! Semantic queries over rendered HTML for tests.
//!
//! The table and form renderers own their markup; tests assert the behavior
//! the markup carries. Each helper reads one stable hook:
//! `tr[id^="row-"]` for rows, `a[aria-label]` for actions,
//! `#{name}-error` for field errors, `[data-filter-name]` for filters.
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
    /// The `Delete` link target, `None` when policy hides it.
    pub delete_href: Option<String>,
    /// The `Delete` POST target carried by `data-row-delete-action`.
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
                .delete_href
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
    for (_, attrs, inner) in tags(html) {
        if attr_value(&attrs, "id").as_deref() == Some(target.as_str()) {
            let text = strip_text(&inner);
            if text.is_empty() {
                return None;
            }
            return Some(text);
        }
    }
    None
}

/// The options of the select filter `name`, or `None` when `name` has no
/// select control.
pub fn filter_options(html: &str, name: &str) -> Option<Vec<FilterOption>> {
    for (tag, attrs, inner) in tags(html) {
        if attr_value(&attrs, "data-filter-name").as_deref() != Some(name) {
            continue;
        }
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
    for (tag, attrs, _) in tags(row_html) {
        if tag == "input" && has_attr(&attrs, "data-row-select") {
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
    let mut delete_href = None;
    let mut delete_action = None;
    for (tag, attrs, _) in tags(row_html) {
        if tag != "a" {
            continue;
        }
        match attr_value(&attrs, "aria-label").as_deref() {
            Some("View") => view_href = attr_value(&attrs, "href"),
            Some("Edit") => edit_href = attr_value(&attrs, "href"),
            Some("Delete") => {
                delete_href = attr_value(&attrs, "href");
                delete_action = attr_value(&attrs, "data-row-delete-action");
            }
            _ => {}
        }
    }
    Row {
        select_value,
        cells,
        actions: RowActions {
            view: view_href,
            edit: edit_href,
            delete_href,
            delete_action,
        },
    }
}

fn href_contains_key(href: &str, key: &str) -> bool {
    let delete = format!("delete={key}");
    href.split(['?', '#', '/', '&']).any(|segment| {
        segment == key
            || segment
                .split('.')
                .next_back()
                .is_some_and(|param| param == delete)
    })
}

fn is_selected(attrs: &Attrs) -> bool {
    attrs.iter().any(|(name, _)| name == "selected")
}

fn has_attr(attrs: &Attrs, name: &str) -> bool {
    attrs.iter().any(|(attr, _)| attr == name)
}

fn attr_value(attrs: &Attrs, name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(attr, _)| attr == name)
        .and_then(|(_, value)| value.clone())
}

fn tags(html: &str) -> Vec<Element> {
    open_tags(html)
        .into_iter()
        .map(|(tag, attrs, open_end)| {
            if is_void(&tag) {
                return (tag, attrs, String::new());
            }
            let close = format!("</{tag}>");
            let after = &html[open_end + 1..];
            match after.find(close.as_str()) {
                Some(close_at) => (tag, attrs, after[..close_at].to_string()),
                None => (tag, attrs, String::new()),
            }
        })
        .collect()
}

fn elements(html: &str, tag: &str) -> Vec<Element> {
    tags(html)
        .into_iter()
        .filter(|(name, _, _)| name == tag)
        .collect()
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
mod tests {
    use super::*;

    const TABLE: &str = r#"<table><thead><tr><th>Name</th></tr></thead><tbody>
<tr id="row-Ada-12345678"><td><input type="checkbox" value="Ada" aria-label="Select row" data-row-select=""></td><td>Ada</td><td><div class="flex"><a href="/admin/dummies/Ada" aria-label="View">V</a><a href="/admin/dummies/Ada/edit" aria-label="Edit">E</a><a href="/admin/dummies?delete=Ada" data-row-delete-action="/admin/dummies/Ada/delete" aria-label="Delete">D</a></div></td></tr>
<tr id="group-Active-87654321"><td colspan="3">Active (1 on this page)</td></tr>
<tr id="row-Ken-abcdef12"><td></td><td>Ken</td><td></td></tr>
<tr><td colspan="3">No records yet</td></tr>
<tr id="row-NoBox-99999999"><td></td><td>NoBox</td><td><div class="flex"><a href="/admin/dummies/NoBox" aria-label="View">V</a></div></td></tr>
<tr id="row-Prefixed-aaaaaaaa"><td></td><td>Prefixed</td><td><div class="flex"><a href="/admin/rel?rel.delete=Prefixed" aria-label="Delete">D</a></div></td></tr>
</tbody></table>"#;

    #[test]
    fn rows_skips_group_headers_and_reads_actions() {
        let found = rows(TABLE);
        assert_eq!(found.len(), 4);
        assert_eq!(found[0].select_value.as_deref(), Some("Ada"));
        assert!(found[0].cells.iter().any(|cell| cell == "Ada"));
        assert_eq!(found[0].actions.view.as_deref(), Some("/admin/dummies/Ada"));
        assert_eq!(
            found[0].actions.edit.as_deref(),
            Some("/admin/dummies/Ada/edit")
        );
        assert_eq!(found[1].select_value, None);
        assert!(found[1].cells.iter().any(|cell| cell == "Ken"));
        assert_eq!(found[1].actions.view, None);
    }

    #[test]
    fn row_actions_finds_by_key() {
        let actions = row_actions(TABLE, "Ada").expect("Ada has actions");
        assert_eq!(actions.view.as_deref(), Some("/admin/dummies/Ada"));
        assert_eq!(actions.edit.as_deref(), Some("/admin/dummies/Ada/edit"));
        assert_eq!(
            actions.delete_action.as_deref(),
            Some("/admin/dummies/Ada/delete")
        );
        let without_checkbox =
            row_actions(TABLE, "NoBox").expect("a row without a checkbox matches by URL");
        assert_eq!(
            without_checkbox.view.as_deref(),
            Some("/admin/dummies/NoBox")
        );
        assert_eq!(without_checkbox.edit, None);
        let prefixed =
            row_actions(TABLE, "Prefixed").expect("a prefixed delete param matches by URL");
        assert_eq!(
            prefixed.delete_href.as_deref(),
            Some("/admin/rel?rel.delete=Prefixed")
        );
        assert_eq!(prefixed.delete_action, None);
        assert_eq!(row_actions(TABLE, "Ad"), None);
        assert_eq!(row_actions(TABLE, "Nobody"), None);
    }

    #[test]
    fn field_error_reads_the_error_slot() {
        let html = r#"<div class="ac-field ac-field--error"><input name="name" aria-invalid="true" aria-describedby="name-error"><div id="name-error" class="ac-error">name is required</div></div>"#;
        assert_eq!(
            field_error(html, "name").as_deref(),
            Some("name is required")
        );
        assert_eq!(field_error(html, "email"), None);
    }

    #[test]
    fn filter_options_reads_values_and_selection() {
        let html = r#"<label>Status<select name="f.status" data-filter-name="status"><option value="">All</option><option value="draft">draft</option><option value="published" selected="">published</option></select></label><label>Created<input type="date" name="f.created_at" data-filter-name="created_at" value="2024-01-15"></label>"#;
        let options = filter_options(html, "status").expect("status has options");
        assert_eq!(options.len(), 3);
        assert!(options.iter().any(|option| option.value == "draft"));
        assert_eq!(
            options
                .iter()
                .find(|option| option.value == "published")
                .map(|option| option.selected),
            Some(true)
        );
        assert_eq!(filter_options(html, "created_at"), None);
        assert_eq!(filter_options(html, "other"), None);
    }

    #[test]
    fn single_quotes_and_entities_decode() {
        let html =
            "<select data-filter-name='status'><option value='a&amp;b'>A &amp; B</option></select>";
        let options = filter_options(html, "status").expect("options");
        assert_eq!(options[0].value, "a&b");
        assert_eq!(options[0].label, "A & B");
    }
}
