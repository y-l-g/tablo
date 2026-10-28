//! Reads a page's `Section` blocks: `[data-section]` for each, `[data-section-title]` for its
//! heading, and a `<details>` without `open` for one the page renders folded.

use super::dom::{attr_value, strip_text, tags_where};

/// One `Section` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The heading's text: the title, then the description.
    pub heading: String,
    /// Whether the section folds its body away on a click of its heading.
    pub collapsible: bool,
    /// Whether the page shows the section's body: always for a section that does not collapse.
    pub open: bool,
    /// The keys the controls in the section post, in document order, each once.
    pub keys: Vec<String>,
}

/// The sections `html` renders, in document order: a nested section's after the one holding it,
/// whose `keys` hold the nested section's too.
pub fn sections(html: &str) -> Vec<Section> {
    tags_where(html, |_, attrs| attr_value(attrs, "data-section").is_some())
        .into_iter()
        .map(|(tag, attrs, inner)| {
            let heading = tags_where(&inner, |_, attrs| {
                attr_value(attrs, "data-section-title").is_some()
            })
            .into_iter()
            .next()
            .map(|(_, _, heading)| strip_text(&heading))
            .unwrap_or_default();
            let mut keys: Vec<String> = Vec::new();
            for (_, attrs, _) in tags_where(&inner, |_, attrs| attr_value(attrs, "name").is_some())
            {
                let key = attr_value(&attrs, "name").unwrap_or_default();
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
            let collapsible = tag == "details";
            Section {
                heading,
                collapsible,
                open: !collapsible || attrs.iter().any(|(name, _)| name == "open"),
                keys,
            }
        })
        .collect()
}
