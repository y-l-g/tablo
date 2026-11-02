//! Reads the summaries under a list's rows: `[data-summary]` names each one's label, and
//! `[data-summary-count]` holds its count.

use super::dom::{attr_value, tags_where};

/// One summary under a list's rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryFigure {
    /// The summary's label, in the request's locale.
    pub label: String,
    /// The records it counts.
    pub count: u64,
}

/// The summaries of every list `html` renders, in document order.
pub fn summaries(html: &str) -> Vec<SummaryFigure> {
    tags_where(html, |_, attrs| attr_value(attrs, "data-summary").is_some())
        .into_iter()
        .filter_map(|(_, attrs, _)| {
            Some(SummaryFigure {
                label: attr_value(&attrs, "data-summary")?,
                count: attr_value(&attrs, "data-summary-count")?.parse().ok()?,
            })
        })
        .collect()
}
