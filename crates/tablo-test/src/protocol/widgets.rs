//! Reads a page's widget grid and its stat tiles: `[data-widget]` names each widget the grid
//! renders, `[data-stat]` marks each tile, whose first two paragraphs hold its label and value.

use super::dom::{attr_value, elements, strip_text, tags_where};

/// The names of the widgets a page's grid renders, in order: `RecentOrdersWidget` is
/// `recent-orders`.
pub fn widgets(html: &str) -> Vec<String> {
    tags_where(html, |_, attrs| attr_value(attrs, "data-widget").is_some())
        .into_iter()
        .filter_map(|(_, attrs, _)| attr_value(&attrs, "data-widget"))
        .collect()
}

/// One stat tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatTile {
    /// What the figure counts.
    pub label: String,
    /// The figure.
    pub value: String,
    /// Where the tile links, if it does.
    pub href: Option<String>,
}

/// The stat tiles `html` renders, in document order.
pub fn stats(html: &str) -> Vec<StatTile> {
    tags_where(html, |_, attrs| {
        attrs.iter().any(|(name, _)| name == "data-stat")
    })
    .into_iter()
    .map(|(tag, attrs, inner)| {
        let mut paragraphs = elements(&inner, "p")
            .into_iter()
            .map(|(_, _, p)| strip_text(&p));
        StatTile {
            label: paragraphs.next().unwrap_or_default(),
            value: paragraphs.next().unwrap_or_default(),
            href: (tag == "a").then(|| attr_value(&attrs, "href")).flatten(),
        }
    })
    .collect()
}
