//! Reads a page's action buttons: whether one confirms before it posts, whether the bulk bar
//! offers every matching record, and which custom actions show disabled with their reason.

use super::dom::{Attrs, Element, attr_value, elements, open_tags, strip_text};

/// A custom action whose button shows disabled, as the record's state refuses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisabledAction {
    /// The action's `NAME`.
    pub name: String,
    /// Why the button is disabled, as the action's `disabled_reason` words it.
    pub reason: String,
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

/// The text of the button posting to `url`, a custom action's route, on a row or a record page,
/// its icon aside. A row's button posts itself; a record page's button sits in the form posting
/// to `url`. `None` when no button posts there.
pub fn action_label(html: &str, url: &str) -> Option<String> {
    let posts_to = |attrs: &Attrs, name: &str| {
        attr_value(attrs, name).is_some_and(|posted| posted.split('?').next() == Some(url))
    };
    let text = |(_, _, inner): Element| strip_text(&inner);
    elements(html, "button")
        .into_iter()
        .find(|(_, attrs, _)| posts_to(attrs, "formaction"))
        .map(text)
        .or_else(|| {
            elements(html, "form")
                .into_iter()
                .find(|(_, attrs, _)| posts_to(attrs, "action"))
                .and_then(|(_, _, inner)| elements(&inner, "button").into_iter().next())
                .map(text)
        })
}

/// Whether the bulk bar offers to select every record the list's query selects, beyond its page.
pub fn selects_matching(html: &str) -> bool {
    open_tags(html).iter().any(|(tag, attrs, _)| {
        tag == "button" && attr_value(attrs, "data-select-matching").is_some()
    })
}

/// The custom actions `html` shows disabled, in document order: a row's, or a record page
/// header's.
pub fn disabled_actions(html: &str) -> Vec<DisabledAction> {
    open_tags(html)
        .into_iter()
        .filter_map(|(_, attrs, _)| {
            Some(DisabledAction {
                name: attr_value(&attrs, "data-disabled-action")?,
                reason: attr_value(&attrs, "title").unwrap_or_default(),
            })
        })
        .collect()
}
