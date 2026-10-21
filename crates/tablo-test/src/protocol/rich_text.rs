//! Reads the rich text values a detail page renders: each one's HTML sits in an element carrying
//! `data-rich-text-value`.

use super::dom::{attr_value, tags_where};

/// The HTML of each rich text value `html` renders, in document order.
pub fn rich_text_values(html: &str) -> Vec<String> {
    tags_where(html, |_, attrs| {
        attr_value(attrs, "data-rich-text-value").is_some()
    })
    .into_iter()
    .map(|(_, _, inner)| inner)
    .collect()
}
