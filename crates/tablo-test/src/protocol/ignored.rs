//! Reads a table's ignored-filters banner: each entry's `data-ignored-filter` names the filter as
//! the request spelled it, and its `data-ignored` names why.

use tablo_core::dom::ignored;

use super::dom::{attr_value, tags_where};

/// A requested filter the table did not apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoredFilter {
    /// The filter's parameters as the request spelled them, such as `f.status=draft`.
    pub filter: String,
    /// Why the table ignored it.
    pub reason: IgnoredReason,
}

/// Why a table ignored a requested filter, as the banner's `data-ignored` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IgnoredReason {
    /// The table declares no filter of that name.
    Unknown,
    /// The filter selects nothing by the value, or refuses it, as it refuses a key naming a
    /// record the user may not see: the two read alike.
    Invalid,
    /// The request carried more filters, values or bytes than the table reads.
    Dropped,
}

/// The filters the banner of `html` lists as ignored, in its order; empty without a banner.
///
/// # Panics
///
/// When an entry names no reason this version knows: a renderer that drops `data-ignored`'s
/// value would otherwise read as any reason.
pub fn ignored_filters(html: &str) -> Vec<IgnoredFilter> {
    tags_where(html, |_, attrs| {
        attr_value(attrs, "data-ignored-filter").is_some()
    })
    .into_iter()
    .filter_map(|(_, attrs, _)| {
        let filter = attr_value(&attrs, "data-ignored-filter")?;
        let reason = match attr_value(&attrs, "data-ignored").as_deref() {
            Some(ignored::UNKNOWN) => IgnoredReason::Unknown,
            Some(ignored::INVALID) => IgnoredReason::Invalid,
            Some(ignored::DROPPED) => IgnoredReason::Dropped,
            other => panic!("the ignored filter names no known reason: {other:?}"),
        };
        Some(IgnoredFilter { filter, reason })
    })
    .collect()
}
