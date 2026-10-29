//! Reads a detail page's entries, where `data-entry` names the column each one shows, and the
//! record pages a record's page links.

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

/// The URLs of the record pages a record's page links, in document order: each link's
/// `data-record-page`.
pub fn record_page_links(html: &str) -> Vec<String> {
    tags_where(html, |_, attrs| {
        attr_value(attrs, "data-record-page").is_some()
    })
    .into_iter()
    .filter_map(|(_, attrs, _)| attr_value(&attrs, "data-record-page"))
    .collect()
}
