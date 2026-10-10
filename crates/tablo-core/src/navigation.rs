//! Sidebar navigation: [`NavigationItem`] and the [`NavTarget`] it points with.

use topcoat::{context::Cx, icon::IconData, view::Attributes};

/// The `href` and runtime-navigation attributes of a link to another panel page.
pub(crate) fn runtime_link(cx: &Cx, href: &str) -> Attributes {
    topcoat::runtime::link_attrs(cx, href.to_string(), topcoat::runtime::prefetch_mode(cx))
}

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
    pub label: String,
    /// Where this entry points.
    pub target: NavTarget,
    /// Sort key for the sidebar.
    pub order: i32,
    /// The icon before the label, if any.
    pub icon: Option<IconData>,
    /// The labelled sidebar group the entry renders in; `None` renders it with the ungrouped
    /// entries, above every group.
    pub group: Option<String>,
}

impl NavigationItem {
    /// A sidebar entry at an explicit `url`.
    pub fn at(label: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            target: NavTarget::Url(url.into()),
            order: 0,
            icon: None,
            group: None,
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
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
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

    /// Whether this item is current for the given request path.
    pub fn is_current_path(&self, current_path: &str) -> bool {
        let Some(url) = self.url() else {
            return false;
        };
        if current_path == url {
            return true;
        }
        current_path
            .strip_prefix(url)
            .is_some_and(|rest| rest.starts_with('/'))
    }
}

#[cfg(test)]
mod tests;
