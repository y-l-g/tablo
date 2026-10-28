//! Reads a page's action buttons: whether one confirms before it posts, whether the bulk bar
//! offers every matching record, and which custom actions show disabled with their reason.

use super::dom::{attr_value, open_tags};

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
