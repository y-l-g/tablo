//! Reads a detail page's entries, where `data-entry` names the column each one shows, and a
//! record's navigation.

use super::dom::{attr_value, strip_text, tags_where};

/// One entry of a detail page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailEntry {
    /// The name of the column the entry shows.
    pub column: String,
    /// The entry's text: its label, then its value.
    pub text: String,
}

/// The entries a detail page renders, in document order, a nested block's in its place.
pub fn detail_entries(html: &str) -> Vec<DetailEntry> {
    tags_where(html, |_, attrs| attr_value(attrs, "data-entry").is_some())
        .into_iter()
        .map(|(_, attrs, inner)| DetailEntry {
            column: attr_value(&attrs, "data-entry").unwrap_or_default(),
            text: strip_text(&inner),
        })
        .collect()
}

/// One tab of a record's navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordNavLink {
    /// Where the tab links.
    pub href: String,
    /// Whether the page is the tab's.
    pub current: bool,
}

/// The tabs of the navigation a record's page renders, in document order: each tab's
/// `data-record-nav`, and whether `aria-current` marks it. Empty without a navigation.
pub fn record_navigation(html: &str) -> Vec<RecordNavLink> {
    tags_where(html, |_, attrs| {
        attr_value(attrs, "data-record-nav").is_some()
    })
    .into_iter()
    .map(|(_, attrs, _)| RecordNavLink {
        href: attr_value(&attrs, "data-record-nav").unwrap_or_default(),
        current: attr_value(&attrs, "aria-current").as_deref() == Some("page"),
    })
    .collect()
}
