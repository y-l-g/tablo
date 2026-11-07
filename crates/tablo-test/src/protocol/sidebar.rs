//! The shell's navigation: the sidebar's entries and groups, the badge an entry shows, an area's
//! link back to the panel, and the topbar's trail.

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

/// One labelled group of the sidebar: its label and the `href` of each entry it lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavGroup {
    pub label: String,
    pub links: Vec<String>,
}

/// The sidebar's groups, in the order it lists them.
pub fn nav_groups(html: &str) -> Vec<NavGroup> {
    tags_where(html, |_, attrs| {
        attr_value(attrs, "data-sidebar").as_deref() == Some("group")
    })
    .into_iter()
    .filter_map(|(_, _, group)| {
        let (_, _, label) = tags_where(&group, |_, attrs| {
            attr_value(attrs, "data-sidebar").as_deref() == Some("group-label")
        })
        .into_iter()
        .next()?;
        Some(NavGroup {
            label: strip_text(&label),
            links: nav_links(&group),
        })
    })
    .collect()
}

/// The `href` of an area's link back to the panel, or `None` outside every area.
pub fn area_back_link(html: &str) -> Option<String> {
    open_tags(html)
        .into_iter()
        .find(|(tag, attrs, _)| tag == "a" && attr_value(attrs, "data-area-back").is_some())
        .and_then(|(_, attrs, _)| attr_value(&attrs, "href"))
}

/// The topbar's trail: each step's text, with its `href` unless it is the page itself.
pub fn trail(html: &str) -> Vec<(String, Option<String>)> {
    let Some((_, _, nav)) = tags_where(html, |tag, attrs| {
        tag == "nav" && attr_value(attrs, "aria-label").as_deref() == Some("breadcrumb")
    })
    .into_iter()
    .next() else {
        return Vec::new();
    };
    tags_where(&nav, |tag, _| tag == "li")
        .into_iter()
        .filter_map(|(_, _, item)| {
            let text = strip_text(&item);
            if text.is_empty() {
                return None;
            }
            let href = open_tags(&item)
                .into_iter()
                .find(|(tag, _, _)| tag == "a")
                .and_then(|(_, attrs, _)| attr_value(&attrs, "href"));
            Some((text, href))
        })
        .collect()
}

/// The `href` of the tenant menu's link to the page `page` names, `profile` for the tenant
/// settings or `registration` for a new tenant; `None` when the menu has no such link.
pub fn tenant_page_link(html: &str, page: &str) -> Option<String> {
    open_tags(html)
        .into_iter()
        .find(|(tag, attrs, _)| {
            tag == "a" && attr_value(attrs, "data-tenant-page").as_deref() == Some(page)
        })
        .and_then(|(_, attrs, _)| attr_value(&attrs, "href"))
}
