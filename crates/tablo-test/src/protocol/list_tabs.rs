//! Reads a list's tab bar: `[data-list-tab]` names each tab's key, `aria-current` marks the one
//! the list shows, and `[data-list-tab-count]` holds a badged tab's count.

use super::dom::{attr_value, tags_where};

/// One tab of a list's tab bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabLink {
    /// The tab's `?tab=` key.
    pub key: String,
    /// Whether the list shows this tab.
    pub active: bool,
    /// The rows the tab holds, when it shows a badge.
    pub count: Option<u64>,
    /// Where the tab links.
    pub href: String,
}

/// The tabs of every list `html` renders, in document order.
pub fn tab_links(html: &str) -> Vec<TabLink> {
    tags_where(html, |tag, attrs| {
        tag == "a" && attr_value(attrs, "data-list-tab").is_some()
    })
    .into_iter()
    .map(|(_, attrs, inner)| TabLink {
        key: attr_value(&attrs, "data-list-tab").unwrap_or_default(),
        active: attr_value(&attrs, "aria-current").as_deref() == Some("page"),
        count: tags_where(&inner, |_, attrs| {
            attr_value(attrs, "data-list-tab-count").is_some()
        })
        .into_iter()
        .next()
        .and_then(|(_, attrs, _)| attr_value(&attrs, "data-list-tab-count")?.parse().ok()),
        href: attr_value(&attrs, "href").unwrap_or_default(),
    })
    .collect()
}
