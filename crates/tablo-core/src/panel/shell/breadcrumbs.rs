//! The top bar's trail: the sidebar entry the request is under, then where in it.

use tablo_ui::{
    breadcrumb, breadcrumb_item, breadcrumb_link, breadcrumb_list, breadcrumb_page,
    breadcrumb_separator,
};
use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use crate::{
    Text,
    navigation::{NavigationItem, current_index},
    panel::state::{Owned, ResourceTrail},
};

/// One step of a trail: its label, and its link unless it is the page the request is on.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Crumb {
    pub(super) label: Text,
    pub(super) href: Option<String>,
    /// Whether the crumb is the page the request is on, which a crumb without a link may not be.
    pub(super) here: bool,
}

impl Crumb {
    fn link(label: impl Into<Text>, href: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            href: Some(href.into()),
            here: false,
        }
    }

    fn here(label: impl Into<Text>) -> Self {
        Self {
            label: label.into(),
            href: None,
            here: true,
        }
    }

    /// A step with no page of its own to link: a record with neither a detail nor an edit page.
    fn text(label: impl Into<Text>) -> Self {
        Self {
            label: label.into(),
            href: None,
            here: false,
        }
    }
}

/// The resource whose pages `current_path` is under, and the segments after its list.
pub(super) struct Located<'a> {
    pub(super) trail: &'a ResourceTrail,
    pub(super) rest: Vec<&'a str>,
}

impl Located<'_> {
    /// The record key the path names, unless it is a page of the list such as `-/create`.
    fn record(&self) -> Option<&str> {
        self.rest
            .first()
            .copied()
            .filter(|segment| *segment != crate::panel::url::DASH_ROUTE_SEGMENT)
    }

    /// The record key of a record page: its detail page, its edit page or one of its own pages,
    /// never an action route such as `delete`, which reads its record only once it passes its
    /// checks.
    fn page_record(&self) -> Option<&str> {
        let page = match self.rest.as_slice() {
            [_] | [_, "edit"] => true,
            [_, slug] => self.trail.pages.iter().any(|(page, _)| page == slug),
            _ => false,
        };
        self.record().filter(|_| page)
    }
}

/// The resource of `trails` whose list `current_path` is, or is under: the longest such list.
pub(super) fn locate<'a>(
    trails: &'a [ResourceTrail],
    current_path: &'a str,
) -> Option<Located<'a>> {
    let trail = trails
        .iter()
        .filter(|trail| {
            current_path
                .strip_prefix(trail.list.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
        .max_by_key(|trail| trail.list.len())?;
    let rest = current_path[trail.list.len()..]
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    Some(Located { trail, rest })
}

/// What the trail read of the records the path names, loaded before it is built.
#[derive(Default)]
pub(super) struct Seen<'a> {
    /// The title of the record the path names, when the request may view it.
    pub(super) title: Option<String>,
    /// The resource a resource the sidebar does not list is placed under.
    pub(super) owner: Option<SeenOwner<'a>>,
}

/// The owner of a resource the sidebar does not list: the resource whose relation lists it.
pub(super) struct SeenOwner<'a> {
    pub(super) trail: &'a ResourceTrail,
    /// The owner record the path's record belongs to, with its title when the request may view
    /// it; `None` when the path names no record holding its owner's key.
    pub(super) record: Option<(String, Option<String>)>,
    /// The relation's tab under the owner record, by the child's slug.
    pub(super) tab: Option<String>,
}

impl Seen<'_> {
    /// The path the sidebar marks its entry by: the owner record's, or else the owner's list, for
    /// a resource the sidebar does not list, and the request's own otherwise.
    pub(super) fn nav_path(&self, current_path: &str) -> String {
        match &self.owner {
            Some(SeenOwner {
                trail,
                record: Some((key, _)),
                ..
            }) => format!("{}/{key}", trail.list),
            Some(owner) => owner.trail.list.clone(),
            None => current_path.to_string(),
        }
    }
}

/// Loads what the trail of `located` shows: the title of the record it names, and, for a resource
/// the sidebar does not list, the owner it is placed under. `load` false reads no record, for a
/// page that failed: it names its owner from the declarations alone.
///
/// Of several resources whose relation lists the resource, the owner is the one whose key the
/// record, or a create page's query, holds; else the first registered.
pub(super) async fn see<'a>(
    cx: &Cx,
    trails: &'a [ResourceTrail],
    located: Option<&Located<'a>>,
    current_query: &str,
    load: bool,
) -> Seen<'a> {
    let Some(located) = located else {
        return Seen::default();
    };
    let facts = match located.page_record().filter(|_| load) {
        Some(key) => (located.trail.record)(cx, key).await,
        None => None,
    };
    let title = facts.as_ref().map(|facts| facts.title.clone());
    if located.trail.listed {
        return Seen { title, owner: None };
    }
    let owners: Vec<(&ResourceTrail, &Owned)> = trails
        .iter()
        .filter(|trail| trail.listed)
        .filter_map(|trail| {
            let owned = trail
                .owns
                .iter()
                .find(|owned| owned.child == located.trail.resource)?;
            Some((trail, owned))
        })
        .collect();
    // A create page opened from an owner's relation table carries the owner's key in its query.
    let query: Vec<(String, String)> = form_urlencoded::parse(current_query.as_bytes())
        .into_owned()
        .collect();
    let held = |foreign_key: &str| -> Option<String> {
        let held = match &facts {
            Some(facts) => facts.values.text(foreign_key).map(str::to_string),
            None => query
                .iter()
                .find(|(key, _)| key == foreign_key)
                .map(|(_, value)| value.clone()),
        };
        held.filter(|key| !key.trim().is_empty())
    };
    let picked = owners.iter().find_map(|(trail, owned)| {
        let key = held(owned.foreign_key.as_deref()?)?;
        Some((*trail, *owned, key))
    });
    let owner = match picked {
        Some((trail, owned, key)) => {
            let title = if load {
                (trail.record)(cx, &key).await.map(|facts| facts.title)
            } else {
                None
            };
            Some(SeenOwner {
                trail,
                record: Some((key, title)),
                tab: owned.tab.clone(),
            })
        }
        None => owners.first().map(|(trail, _)| SeenOwner {
            trail,
            record: None,
            tab: None,
        }),
    };
    Seen { title, owner }
}

/// The trail to `current_path` under `current_query`, `located` under a resource's list and
/// reading what `seen` loaded: the sidebar entry it is under, as the sidebar marks it, then the
/// page within the entry. Empty when no entry holds the path.
///
/// Under a resource's list, a path names a resource's page: `/admin/posts` is `Blog Posts`;
/// `/admin/posts/-/create` adds `Create`, `/admin/posts/7` adds the record's title, and
/// `/admin/posts/7/edit` adds the title, linked to the record's detail page, then `Edit`. A
/// deeper path under a record names the page by its label, else by its last segment. A record
/// the request may not view reads `View`.
///
/// A resource the sidebar does not list sits under its owner: the owner's entry, the owner record
/// and its relation's tab lead the record's own steps, so an expense line's edit page reads
/// `Expenses › EXP-0002 › Expense lines › Monitor › Edit`.
///
/// Under any other entry, the home page's included, a deeper path names its last segment:
/// `/admin/reports` is `Dashboard › Reports`.
pub(super) fn trail(
    nav_items: &[NavigationItem],
    located: Option<&Located<'_>>,
    seen: &Seen<'_>,
    current_path: &str,
    current_query: &str,
) -> Vec<Crumb> {
    let nav_path = seen.nav_path(current_path);
    let Some((item, path)) = entry(nav_items, &nav_path, current_query) else {
        return Vec::new();
    };
    let entry = || Crumb::link(item.label.clone(), path);
    let Some(located) =
        located.filter(|located| !located.trail.listed || located.trail.list == path)
    else {
        let rest = current_path
            .strip_prefix(path)
            .unwrap_or_default()
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>();
        return match rest.last() {
            None => vec![Crumb::here(item.label.clone())],
            Some(last) => vec![entry(), Crumb::here(humanize(last))],
        };
    };
    let mut crumbs = vec![entry()];
    match &seen.owner {
        Some(owner) => {
            if let Some((key, title)) = &owner.record {
                crumbs.push(record_crumb(owner.trail, key, title.clone()));
                if let Some(slug) = &owner.tab {
                    let label = owner
                        .trail
                        .pages
                        .iter()
                        .find(|(page, _)| page == slug)
                        .map_or_else(|| located.trail.label.clone(), |(_, label)| label.clone());
                    crumbs.push(Crumb::link(
                        label,
                        format!("{}/{key}/{slug}", owner.trail.list),
                    ));
                }
            }
            if located.rest.is_empty() {
                crumbs.push(Crumb::here(located.trail.label.clone()));
                return crumbs;
            }
        }
        None if located.rest.is_empty() => return vec![Crumb::here(item.label.clone())],
        None => {}
    }
    let resource = located.trail;
    let title = seen.title.clone();
    match located.rest.as_slice() {
        [] => {}
        ["-", "create"] => crumbs.push(Crumb::here(Text::message("tablo-crumb-create"))),
        ["-", action] => crumbs.push(Crumb::here(humanize(action))),
        [_record] => crumbs.push(Crumb::here(
            title.map_or_else(|| Text::message("tablo-crumb-view"), Text::from),
        )),
        [record, "edit"] => {
            if let Some(title) = title {
                crumbs.push(record_crumb(resource, record, Some(title)));
            }
            crumbs.push(Crumb::here(Text::message("tablo-crumb-edit")));
        }
        [record, .., last] => {
            let label = resource
                .pages
                .iter()
                .find(|(slug, _)| slug == last)
                .map_or_else(|| Text::from(humanize(last)), |(_, label)| label.clone());
            if title.is_some() || resource.viewed {
                crumbs.push(record_crumb(resource, record, title));
            }
            crumbs.push(Crumb::here(label));
        }
    }
    crumbs
}

/// The step naming `resource`'s record `key` by its `title`, else `View`: linked to its detail
/// page, else its edit page, else to nothing.
fn record_crumb(resource: &ResourceTrail, key: &str, title: Option<String>) -> Crumb {
    let label = title.map_or_else(|| Text::message("tablo-crumb-view"), Text::from);
    if resource.viewed {
        Crumb::link(label, format!("{}/{key}", resource.list))
    } else if resource.edited {
        Crumb::link(
            label,
            format!(
                "{}/{key}/{}",
                resource.list,
                crate::panel::url::EDIT_ROUTE_SEGMENT
            ),
        )
    } else {
        Crumb::text(label)
    }
}

/// `trail` under the area labelled `label` at `url`: the area first, linked, unless the trail is
/// empty.
pub(super) fn within(label: Text, url: &str, trail: Vec<Crumb>) -> Vec<Crumb> {
    if trail.is_empty() {
        return trail;
    }
    std::iter::once(Crumb::link(label, url))
        .chain(trail)
        .collect()
}

/// The trail to a page that failed at `nav_path`, the path the sidebar marks its entry by, under
/// `current_query`: the sidebar entry it is under, then `label`, the failure.
pub(super) fn failed(
    nav_items: &[NavigationItem],
    nav_path: &str,
    current_query: &str,
    label: impl Into<Text>,
) -> Vec<Crumb> {
    let mut crumbs: Vec<Crumb> = entry(nav_items, nav_path, current_query)
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

/// A path segment as a label: `audit-log` reads `Audit log`, and `Q1%20sales` reads `Q1 sales`.
fn humanize(segment: &str) -> String {
    let decoded = percent_encoding::percent_decode_str(segment).decode_utf8_lossy();
    let words = decoded.replace(['-', '_'], " ");
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
        breadcrumb(
            attrs: topcoat::view::attributes! { class="max-sm:hidden" },
            breadcrumb_list(
                for (index, crumb) in crumbs.into_iter().enumerate() {
                    let label = crumb.label.localize(cx);
                    // A long record title stops short rather than push the bar's controls away.
                    let label = view! {
                        <span class="block max-w-56 truncate" title=(label.clone())>
                            (label)
                        </span>
                    };
                    breadcrumb_item(
                        if let Some(href) = crumb.href {
                            breadcrumb_link(
                                attrs: crate::navigation::runtime_link(cx, &href),
                                (label)
                            )
                        } else if crumb.here {
                            breadcrumb_page((label))
                        } else {
                            (label)
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
