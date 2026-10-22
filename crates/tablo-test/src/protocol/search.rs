//! Reads the panel's global search out of a rendered page: `[data-global-search]` around the
//! topbar's box and its form, `section[data-search-group]` for each resource's results, and
//! `a[data-search-hit]` for each result, with its title in the first `span` and its details as
//! `dt`/`dd` pairs.

use super::dom::{attr_value, strip_text, tags_where};

/// One resource's results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchGroup {
    /// The resource's plural label, which heads the results.
    pub label: String,
    pub results: Vec<SearchResult>,
}

/// One record a global search lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    /// The page the result opens.
    pub href: String,
    /// Each `(label, value)` under the title.
    pub details: Vec<(String, String)>,
}

/// The results `html` lists, by resource in document order: the results page's, or the topbar
/// box's.
pub fn search_results(html: &str) -> Vec<SearchGroup> {
    tags_where(html, |tag, attrs| {
        tag == "section" && attr_value(attrs, "data-search-group").is_some()
    })
    .into_iter()
    .map(|(_, attrs, inner)| SearchGroup {
        label: attr_value(&attrs, "data-search-group").unwrap_or_default(),
        results: tags_where(&inner, |tag, attrs| {
            tag == "a" && attr_value(attrs, "data-search-hit").is_some()
        })
        .into_iter()
        .map(|(_, attrs, inner)| SearchResult {
            title: tags_where(&inner, |tag, _| tag == "span")
                .first()
                .map(|(_, _, text)| strip_text(text))
                .unwrap_or_default(),
            href: attr_value(&attrs, "href").unwrap_or_default(),
            details: tags_where(&inner, |tag, _| tag == "dt")
                .into_iter()
                .zip(tags_where(&inner, |tag, _| tag == "dd"))
                .map(|((_, _, label), (_, _, value))| {
                    (
                        strip_text(&label).trim_end_matches(':').to_string(),
                        strip_text(&value),
                    )
                })
                .collect(),
        })
        .collect(),
    })
    .collect()
}

/// Where the topbar's search box submits its term, or `None` when the page shows no box.
pub fn search_box(html: &str) -> Option<String> {
    let (_, _, inner) = tags_where(html, |tag, attrs| {
        tag == "div" && attr_value(attrs, "data-global-search").is_some()
    })
    .into_iter()
    .next()?;
    tags_where(&inner, |tag, _| tag == "form")
        .first()
        .and_then(|(_, attrs, _)| attr_value(attrs, "action"))
}
