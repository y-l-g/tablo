//! Reads a page's filter bar: the `[data-page-filters]` form, what its controls submit as they
//! stand, and its `[data-page-filters-reset]` link.

use super::dom::{attr_value, elements, is_selected, open_tags, tags_where};

/// A page's filter bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageFilterBar {
    /// Each `(name, value)` the form submits as it stands, in document order: the request's other
    /// parameters it carries, each select's selected option and each date input's day, blank
    /// ones included.
    pub submits: Vec<(String, String)>,
    /// Where the reset link leads, shown once the request holds a value.
    pub reset: Option<String>,
}

/// The filter bar `html` renders, or `None` without one.
pub fn page_filter_bar(html: &str) -> Option<PageFilterBar> {
    let (_, _, form) = tags_where(html, |tag, attrs| {
        tag == "form" && attr_value(attrs, "data-page-filters").is_some()
    })
    .into_iter()
    .next()?;
    let mut submits: Vec<(usize, String, String)> = open_tags(&form)
        .into_iter()
        .filter(|(tag, _, _)| tag == "input")
        .filter_map(|(_, attrs, at)| {
            Some((
                at,
                attr_value(&attrs, "name")?,
                attr_value(&attrs, "value").unwrap_or_default(),
            ))
        })
        .collect();
    let selects = open_tags(&form)
        .into_iter()
        .filter(|(tag, _, _)| tag == "select")
        .map(|(_, attrs, at)| (at, attr_value(&attrs, "name")));
    for ((at, name), (_, _, inner)) in selects.zip(elements(&form, "select")) {
        let Some(name) = name else { continue };
        let options = elements(&inner, "option");
        let chosen = options
            .iter()
            .find(|(_, attrs, _)| is_selected(attrs))
            .or(options.first())
            .and_then(|(_, attrs, _)| attr_value(attrs, "value"))
            .unwrap_or_default();
        submits.push((at, name, chosen));
    }
    submits.sort_by_key(|(at, _, _)| *at);
    let reset = tags_where(&form, |tag, attrs| {
        tag == "a" && attr_value(attrs, "data-page-filters-reset").is_some()
    })
    .into_iter()
    .next()
    .and_then(|(_, attrs, _)| attr_value(&attrs, "href"));
    Some(PageFilterBar {
        submits: submits
            .into_iter()
            .map(|(_, name, value)| (name, value))
            .collect(),
        reset,
    })
}
