//! The top bar's trail: the sidebar entry the request is under, then where in it.

use tablo_ui::{
    breadcrumb, breadcrumb_item, breadcrumb_link, breadcrumb_list, breadcrumb_page,
    breadcrumb_separator,
};
use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use crate::navigation::{NavigationItem, current_index};

/// One step of a trail: its label, and its link unless it is the page the request is on.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Crumb {
    pub(super) label: String,
    pub(super) href: Option<String>,
}

impl Crumb {
    fn link(label: impl Into<String>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: Some(href.into()),
        }
    }

    fn here(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: None,
        }
    }
}

/// The trail to `current_path` under `current_query`: the sidebar entry it is under, as the
/// sidebar marks it, then the page within the entry. Empty when no entry holds the path.
///
/// Under a resource's list, one of `resource_lists`, a path names a resource's page:
/// `/admin/posts` is `Blog Posts`; `/admin/posts/-/create` adds `Create`, `/admin/posts/7`
/// adds `View`, and `/admin/posts/7/edit` adds `Edit`. A deeper path under a record links the
/// record's `View`, then names its last segment. Under any other entry, the home page's
/// included, a deeper path names its last segment: `/admin/reports` is `Dashboard › Reports`.
pub(super) fn trail(
    nav_items: &[NavigationItem],
    resource_lists: &[&str],
    current_path: &str,
    current_query: &str,
) -> Vec<Crumb> {
    let Some((item, path)) = entry(nav_items, current_path, current_query) else {
        return Vec::new();
    };
    let rest: Vec<&str> = current_path[path.len()..]
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let entry = || Crumb::link(item.label.clone(), path);
    if !resource_lists.contains(&path) {
        return match rest.last() {
            None => vec![Crumb::here(item.label.clone())],
            Some(last) => vec![entry(), Crumb::here(humanize(last))],
        };
    }
    match rest.as_slice() {
        [] => vec![Crumb::here(item.label.clone())],
        ["-", "create"] => vec![entry(), Crumb::here("Create")],
        ["-", action] => vec![entry(), Crumb::here(humanize(action))],
        [_record] => vec![entry(), Crumb::here("View")],
        [_record, "edit"] => vec![entry(), Crumb::here("Edit")],
        [record, .., last] => vec![
            entry(),
            Crumb::link("View", format!("{path}/{record}")),
            Crumb::here(humanize(last)),
        ],
    }
}

/// The trail to a page that failed at `current_path` under `current_query`: the sidebar entry it
/// is under, then `label`, the failure.
pub(super) fn failed(
    nav_items: &[NavigationItem],
    current_path: &str,
    current_query: &str,
    label: &str,
) -> Vec<Crumb> {
    let mut crumbs: Vec<Crumb> = entry(nav_items, current_path, current_query)
        .map(|(item, path)| Crumb::link(item.label.clone(), path))
        .into_iter()
        .collect();
    crumbs.push(Crumb::here(label));
    crumbs
}

/// The sidebar entry the request is under, as the sidebar marks it, with its path.
fn entry<'a>(
    nav_items: &'a [NavigationItem],
    current_path: &str,
    current_query: &str,
) -> Option<(&'a NavigationItem, &'a str)> {
    let item = &nav_items[current_index(nav_items, current_path, current_query)?];
    Some((item, item.path()?))
}

/// A path segment as a label: `audit-log` reads `Audit log`.
fn humanize(segment: &str) -> String {
    let words = segment.replace(['-', '_'], " ");
    let mut chars = words.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Render `crumbs`, or nothing for an empty trail. Below `sm` the top bar has no room for it.
pub(super) fn render(cx: &Cx, crumbs: Vec<Crumb>) -> BoxView<'_> {
    if crumbs.is_empty() {
        return ().boxed();
    }
    let last = crumbs.len() - 1;
    view! {
        cx =>
        breadcrumb(
            attrs: topcoat::view::attributes! { class="max-sm:hidden" },
            breadcrumb_list(
                for (index, crumb) in crumbs.into_iter().enumerate() {
                    breadcrumb_item(
                        if let Some(href) = crumb.href {
                            breadcrumb_link(
                                attrs: crate::navigation::runtime_link(cx, &href),
                                (crumb.label)
                            )
                        } else {
                            breadcrumb_page((crumb.label))
                        }
                    )
                    if index < last {
                        breadcrumb_separator()
                    }
                }
            )
        )
    }
    .boxed()
}

#[cfg(test)]
mod tests;
