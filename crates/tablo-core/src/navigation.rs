//! Sidebar navigation: [`NavigationItem`] and the [`NavTarget`] it points with.

use topcoat::{context::Cx, icon::IconData, view::Attributes};

/// The `href` and runtime-navigation attributes of a link to another panel page.
pub(crate) fn runtime_link(cx: &Cx, href: &str) -> Attributes {
    topcoat::runtime::link_attrs(cx, href.to_string(), topcoat::runtime::prefetch_mode(cx))
}

/// A dropdown menu item that navigates: the registry item's look on a link.
pub(crate) const MENU_LINK_CLASS: &str = "flex w-full items-center gap-2 rounded-md px-2 py-1.5 \
     text-left text-sm whitespace-nowrap outline-none hover:bg-foreground/5 \
     focus-visible:bg-foreground/5 active:bg-foreground/10 [&_svg]:size-4 [&_svg]:shrink-0";

/// Where a sidebar entry points.
#[derive(Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum NavTarget {
    /// No URL yet; the owning Panel resolves it.
    #[default]
    Derived,
    /// An explicit URL.
    Url(String),
}

impl NavTarget {
    /// The URL this target names.
    pub fn url(&self) -> Option<&str> {
        match self {
            Self::Derived => None,
            Self::Url(url) => Some(url),
        }
    }
}

impl std::fmt::Debug for NavTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Derived => f.write_str("Derived"),
            Self::Url(url) => f.debug_tuple("Url").field(url).finish(),
        }
    }
}

/// A sidebar entry: a resource's, from its [`ResourceDef`](super::ResourceDef), or a `Page`'s.
#[derive(Clone, Debug, Default)]
pub struct NavigationItem {
    /// The entry's text.
    pub label: crate::Text,
    /// Where this entry points.
    pub target: NavTarget,
    /// Sort key for the sidebar.
    pub order: i32,
    /// The icon before the label, if any.
    pub icon: Option<IconData>,
    /// The labelled sidebar group the entry renders in; `None` renders it with the ungrouped
    /// entries, above every group.
    pub group: Option<crate::Text>,
    /// The text of the badge after the label, if any.
    pub badge: Option<String>,
}

impl NavigationItem {
    /// A sidebar entry at an explicit `url`, kept as written.
    ///
    /// The entry whose URL is the longest match for the current path is marked active. Of the
    /// entries sharing that path, the one whose query the request carries wins, then the one
    /// naming more of that query, then the first declared.
    pub fn at(label: impl Into<crate::Text>, url: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            target: NavTarget::Url(url.into()),
            order: 0,
            icon: None,
            group: None,
            badge: None,
        }
    }

    /// This entry with `icon` before its label.
    #[must_use]
    pub fn icon(mut self, icon: IconData) -> Self {
        self.icon = Some(icon);
        self
    }

    /// This entry in the sidebar group labelled `group`. Groups render in the order of their
    /// first entry, after the ungrouped entries.
    #[must_use]
    pub fn group(mut self, group: impl Into<crate::Text>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// This entry with a badge reading `text` after its label, such as `"New"`; a blank `text`
    /// is ignored. A resource's [`navigation_badge`](crate::ResourceDef::navigation_badge) counts
    /// its rows for one instead.
    #[must_use]
    pub fn badge(mut self, text: impl Into<String>) -> Self {
        let text = text.into().trim().to_string();
        if !text.is_empty() {
            self.badge = Some(text);
        }
        self
    }

    /// Resolve a [`NavTarget::Derived`] entry to `url`.
    pub(crate) fn resolved(mut self, url: &str) -> Self {
        if matches!(self.target, NavTarget::Derived) {
            self.target = NavTarget::Url(url.to_string());
        }
        self
    }

    /// The URL this entry points at.
    pub fn url(&self) -> Option<&str> {
        self.target.url()
    }

    /// The path this entry points at, without the query or fragment it carries.
    pub(crate) fn path(&self) -> Option<&str> {
        let url = self.url()?;
        let path = url.split_once(['?', '#']).map_or(url, |(path, _)| path);
        (!path.is_empty()).then_some(path)
    }

    /// Whether this item is current for the given request path, which carries no query or
    /// fragment: an entry's own are the state it points with, not part of where it points.
    pub fn is_current_path(&self, current_path: &str) -> bool {
        let Some(path) = self.path() else {
            return false;
        };
        if current_path == path {
            return true;
        }
        current_path
            .strip_prefix(path)
            .is_some_and(|rest| rest.starts_with('/'))
    }

    /// The query the entry points with: the text between its `?` and any fragment.
    fn query(&self) -> &str {
        let url = self.url().unwrap_or_default();
        let before_fragment = url.split_once('#').map_or(url, |(before, _)| before);
        before_fragment
            .split_once('?')
            .map_or("", |(_, query)| query)
    }

    /// The `&`-separated parameters the entry's query names.
    fn query_pairs(&self) -> impl Iterator<Item = &str> {
        self.query().split('&').filter(|pair| !pair.is_empty())
    }

    /// Whether `request_query` carries every parameter the entry's query names.
    pub(crate) fn query_held(&self, request_query: &str) -> bool {
        self.query_pairs()
            .all(|pair| request_query.split('&').any(|held| held == pair))
    }
}

/// The index of the entry a request is under, given the request's path and query. Entries whose
/// path holds `path` compete, and the longest path wins. Of the entries sharing it, the one whose
/// query `query` carries wins, then the one naming more of that query, then the first declared.
pub(crate) fn current_index(
    nav_items: &[NavigationItem],
    path: &str,
    query: &str,
) -> Option<usize> {
    nav_items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.is_current_path(path))
        .min_by_key(|(_, item)| {
            (
                std::cmp::Reverse(item.path().map_or(0, str::len)),
                !item.query_held(query),
                std::cmp::Reverse(item.query_pairs().count()),
            )
        })
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests;
