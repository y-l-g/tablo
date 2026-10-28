//! Reads a detail page's entries: `data-entry` names the column each one shows.

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
