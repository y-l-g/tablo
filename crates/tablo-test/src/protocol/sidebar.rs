//! The sidebar's entries: the links the shell lists, and the badge an entry shows.

use super::dom::{Attrs, attr_value, open_tags, strip_text, tags_where};

/// The badge text of the sidebar entry linking to `href`, or `None` when that entry shows no
/// badge or no entry links there.
pub fn nav_badge(html: &str, href: &str) -> Option<String> {
    let menu = |name: &'static str| {
        move |_: &str, attrs: &Attrs| attr_value(attrs, "data-sidebar").as_deref() == Some(name)
    };
    let (_, _, entry) = tags_where(html, menu("menu-item"))
        .into_iter()
        .find(|(_, _, inner)| {
            open_tags(inner).iter().any(|(tag, attrs, _)| {
                tag == "a" && attr_value(attrs, "href").as_deref() == Some(href)
            })
        })?;
    let (_, _, badge) = tags_where(&entry, menu("menu-badge")).into_iter().next()?;
    Some(strip_text(&badge))
}

/// The `href` of each sidebar entry that links somewhere, in the order the sidebar lists them.
pub fn nav_links(html: &str) -> Vec<String> {
    open_tags(html)
        .into_iter()
        .filter(|(tag, attrs, _)| {
            tag == "a" && attr_value(attrs, "data-sidebar").as_deref() == Some("menu-button")
        })
        .filter_map(|(_, attrs, _)| attr_value(&attrs, "href"))
        .collect()
}
