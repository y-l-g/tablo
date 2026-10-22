//! Reads values back out of rendered responses: bodies, cookies, form data,
//! and the semantic HTML queries over tables, fields, filters, the ignored-filters banner, the
//! import page, rich text values, and the notification bell.

mod bell;
pub mod dom;
mod ignored;
mod import;
mod rich_text;

pub use bell::{Bell, BellNotification, bell};
pub use dom::{
    EmptyReason, EmptyTable, ErrorKind, FilterOption, Group, PickerColumn, Row, RowActions,
    column_picker, columns, confirms_first, empty_table, field_error, field_error_message,
    filter_options, find_href_with, groups, nav_badge, row_actions, row_keys, rows,
};
use http_body_util::BodyExt;
pub use ignored::{IgnoredFilter, IgnoredReason, ignored_filters};
pub use import::{ImportError, ImportRefusal, import_columns, import_errors, import_refusal};
pub use rich_text::rich_text_values;
use tablo_core::Notification;
use topcoat::router::Body;

/// One `Cookie` header value from `(name, value)` pairs, or `None` when empty.
pub fn cookie_header<'a>(cookies: impl IntoIterator<Item = (&'a str, &'a str)>) -> Option<String> {
    let mut jar = String::new();
    for (name, value) in cookies {
        if !jar.is_empty() {
            jar.push_str("; ");
        }
        jar.push_str(name);
        jar.push('=');
        jar.push_str(value);
    }
    (!jar.is_empty()).then_some(jar)
}

/// Collect a response body as bytes.
pub async fn body_bytes(response: http::Response<Body>) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes()
        .to_vec()
}

/// Collect a response body as a lossy UTF-8 string.
pub async fn body_string(response: http::Response<Body>) -> String {
    String::from_utf8_lossy(&body_bytes(response).await).into_owned()
}

/// URL-encode `(key, value)` pairs into an urlencoded form body.
pub fn form_body(pairs: &[(&str, &str)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

/// Builds a multipart body, one part per entry: `None` is a text part,
/// `Some("")` the browser's "no file chosen" part, `Some(name)` a chosen file.
///
/// No part carries a `Content-Type`: the panel tells file parts from text parts by the
/// `filename` parameter alone. A test asserting on an upload's content type builds its own body.
pub fn multipart_body(boundary: &str, parts: &[(&str, Option<&str>, &str)]) -> String {
    let mut body = String::new();
    for (name, filename, content) in parts {
        body.push_str(&format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\""
        ));
        if let Some(filename) = filename {
            body.push_str(&format!("; filename=\"{filename}\""));
        }
        body.push_str("\r\n\r\n");
        body.push_str(content);
        body.push_str("\r\n");
    }
    body.push_str(&format!("--{boundary}--\r\n"));
    body
}

/// Reads the value the named `<input>`s post when the form submits untouched, in either attribute
/// order or quote style: the last one's `value`, skipping a radio or checkbox left unchecked. A
/// toggle reads `true` or its hidden `false`, and a radio choice its checked option, or the hidden
/// blank before them when none is checked.
pub fn input_value(html: &str, name: &str) -> Option<String> {
    let double = format!("name=\"{name}\"");
    let single = format!("name='{name}'");
    let mut posted = None;
    for tag in html.split('<').skip(1) {
        if !tag.starts_with("input") || (!tag.contains(&double) && !tag.contains(&single)) {
            continue;
        }
        let Some(end) = tag.find('>') else {
            continue;
        };
        let attrs = &tag[..end];
        let checkable = [
            "type=\"radio\"",
            "type=\"checkbox\"",
            "type='radio'",
            "type='checkbox'",
        ]
        .iter()
        .any(|kind| attrs.contains(kind));
        if checkable && !has_attribute(attrs, "checked") {
            continue;
        }
        for (prefix, term) in [("value=\"", '"'), ("value='", '\'')] {
            if let Some(start) = attrs.find(prefix) {
                let rest = &attrs[start + prefix.len()..];
                if let Some(end) = rest.find(term) {
                    // Unescape the matching quote entity the encoder may emit.
                    let raw = &rest[..end];
                    posted = Some(raw.replace("&quot;", "\"").replace("&#x27;", "'"));
                    break;
                }
            }
        }
    }
    posted
}

/// Whether the opening tag `attrs` carries the attribute `name`, valued or bare, skipping quoted
/// values so a class naming `checked:` does not count.
fn has_attribute(attrs: &str, name: &str) -> bool {
    let mut rest = attrs;
    while let Some(at) = rest.find(|c: char| !c.is_whitespace()) {
        rest = &rest[at..];
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '=' || c == '/')
            .unwrap_or(rest.len());
        if &rest[..end] == name {
            return true;
        }
        rest = &rest[end..];
        if let Some(value) = rest.strip_prefix('=') {
            rest = match value.chars().next() {
                Some(quote @ ('"' | '\'')) => value[1..]
                    .find(quote)
                    .map_or("", |close| &value[close + 2..]),
                _ => value
                    .find(char::is_whitespace)
                    .map_or("", |space| &value[space..]),
            };
        } else if let Some(after) = rest.strip_prefix('/') {
            rest = after;
        }
    }
    false
}

/// Names the session cookie Topcoat's default token store writes.
pub const SESSION_COOKIE: &str = "__Host-session";

/// The `(name, value)` pairs a response's `Set-Cookie` headers carry.
pub fn response_cookies(response: &http::Response<Body>) -> Vec<(String, String)> {
    response
        .headers()
        .get_all(http::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .filter_map(|pair| pair.split_once('='))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect()
}

/// The full `Set-Cookie` header for `name`, so tests can assert attributes.
pub fn set_cookie_header(response: &http::Response<Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get_all(http::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&format!("{name}=")))
        .map(str::to_string)
}

/// The session cookie value a response set, if any.
pub fn session_cookie_value(response: &http::Response<Body>) -> Option<String> {
    response_cookies(response)
        .into_iter()
        .find(|(name, _)| name == SESSION_COOKIE)
        .map(|(_, value)| value)
}

/// The flash notification a response set for the next page, decoded.
pub fn notification(response: &http::Response<Body>) -> Option<Notification> {
    notification_in(&response_cookies(response))
}

/// The flash notification among a response's `(name, value)` cookie pairs, decoded.
pub(crate) fn notification_in(cookies: &[(String, String)]) -> Option<Notification> {
    let (_, value) = cookies
        .iter()
        .find(|(name, _)| name.ends_with("tablo_notification"))?;
    let json = percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .ok()?;
    serde_json::from_str(&json).ok()
}

/// Whether the edit form refuses the save because another save changed the record since the form
/// opened: it renders an element carrying `data-stale`.
pub fn stale_form(html: &str) -> bool {
    !dom::tags_where(html, |_, attrs| {
        attrs.iter().any(|(attr, _)| attr == "data-stale")
    })
    .is_empty()
}
