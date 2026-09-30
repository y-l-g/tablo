//! Sidebar navigation: [`NavigationItem`] and the [`NavTarget`] it points with.

use topcoat::{context::Cx, view::Attributes};

use super::Resource;

/// The `href` and runtime-navigation attributes of a link to another panel
/// page (GH #395).
///
/// The runtime swaps the page in without a document load. The mode is the
/// context's [`prefetch_mode`](topcoat::runtime::prefetch_mode), which the
/// panel's router sets to never.
pub(crate) fn runtime_link(cx: &Cx, href: &str) -> Attributes {
    topcoat::runtime::link_attrs(cx, href.to_string(), topcoat::runtime::prefetch_mode(cx))
}

/// Where a sidebar entry points.
///
/// A [`Resource`] cannot name its own URL: [`Resource::navigation`] takes no
/// `Cx` and no prefix, so the entry it declares by default carries no URL at
/// all — [`NavTarget::Derived`] — and the [`Panel`](crate::panel::Panel) that
/// owns the item resolves it from its own mount prefix plus the resource's
/// [`slug`](Resource::slug). [`NavTarget::Url`] is a URL its author wrote out,
/// and a Panel passes it through untouched.
#[derive(Clone, Default, PartialEq, Eq)]
pub enum NavTarget {
    /// No URL yet: the owning Panel resolves it to `{prefix}/{slug}` of the
    /// resource whose `navigation()` declared this item. What
    /// [`NavigationItem::for_resource`] — and so the default
    /// [`Resource::navigation`] — returns.
    #[default]
    Derived,
    /// An explicit URL: a custom path, a query view, another panel's mount.
    /// Active state is string matching (exact, or a slash-boundary prefix).
    Url(String),
}

impl NavTarget {
    /// The URL this target names, or `None` while it is still
    /// [`Self::Derived`] — i.e. before the owning Panel has resolved it.
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

/// Sidebar entry derived from a `Resource` (see `CONTEXT.md`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NavigationItem {
    pub label: String,
    /// Where this entry points. [`NavTarget::Derived`] until the owning Panel
    /// resolves it — see [`NavTarget`]. Build items with
    /// [`NavigationItem::for_resource`] or [`NavigationItem::at`] rather than
    /// spelling the variant out.
    pub target: NavTarget,
    /// Sort key for the sidebar: items render in stable `order`
    /// order, so declaration order breaks ties. Resources declare in
    /// `Panel::resource` order (all default `0`); a
    /// [`Resource::navigation`] override interleaves by setting a lower value,
    /// e.g. `NavigationItem { order: -1, ..NavigationItem::for_resource::<Self>() }`
    /// pins above the resources.
    pub order: i32,
}

impl NavigationItem {
    /// The default sidebar entry for `R`: its
    /// [`navigation_label`](Resource::navigation_label), and no URL yet.
    ///
    /// This is what [`Resource::navigation`] returns unless overridden — and
    /// what an override decorates, e.g.
    /// `NavigationItem { order: -1, ..NavigationItem::for_resource::<Self>() }`.
    /// The resource cannot know where it is mounted, so the URL stays
    /// [`NavTarget::Derived`] until the owning [`Panel`](crate::panel::Panel)
    /// resolves it; an entry declared here can therefore never link at a mount
    /// the resource guessed.
    pub fn for_resource<R: Resource>() -> Self {
        Self {
            label: R::navigation_label(),
            target: NavTarget::Derived,
            order: 0,
        }
    }

    /// A sidebar entry at an explicit `url`.
    ///
    /// Active state is string matching: exact path, or a slash-boundary prefix,
    /// so `/admin/users` is current on `/admin/users/create`.
    pub fn at(label: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            target: NavTarget::Url(url.into()),
            order: 0,
        }
    }

    /// Resolve a [`NavTarget::Derived`] entry against the Panel that owns it
    /// leaving an explicit target untouched.
    ///
    /// [`Resource::navigation`] cannot know its panel — it takes no `Cx` and no
    /// prefix — so the entry it declares carries no URL. The Panel consumes it
    /// through `Panel::resource`, which passes its own prefix and the
    /// resource's mount slug; there is no panel-root case, since nothing
    /// generates an entry without a resource behind it.
    ///
    /// There is no guessing here: a URL an author wrote out — including one
    /// that happens to look like `/admin/{slug}` — is a different
    /// [`NavTarget`] variant and is never rewritten.
    pub(crate) fn resolved(mut self, prefix: &str, slug: &str) -> Self {
        if matches!(self.target, NavTarget::Derived) {
            self.target = NavTarget::Url(format!("{}/{slug}", mount(prefix)));
        }
        self
    }

    /// The URL this entry points at, or `None` while it is unresolved — i.e.
    /// still [`NavTarget::Derived`], not yet handed to a Panel. Panel-owned
    /// items are always resolved (`Panel::resource`).
    pub fn url(&self) -> Option<&str> {
        self.target.url()
    }

    /// Whether this item is current for the given request path (without query):
    /// an exact match, or a prefix match on a slash boundary (so
    /// `/admin/users` is active on `/admin/users/create` but not on
    /// `/admin/userships`). Uniform for every item — since resources mount at
    /// `{prefix}/{slug}`, no generated item points at the bare panel
    /// prefix.
    ///
    /// This is the whole active-state contract: `Panel::render_shell` takes the
    /// request path as a parameter, so it can judge an item without a `Cx` (and
    /// the shell stays testable without a full `http::request::Parts` in `Cx`).
    /// It is the only entry point to that judgment.
    pub fn is_current_path(&self, current_path: &str) -> bool {
        let Some(url) = self.url() else {
            // Unresolved: no URL to be current for.
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

/// The mount a panel prefix normalises to: `/admin` when it is empty — the same
/// rule [`Panel::new`](crate::panel::Panel::new) applies.
fn mount(prefix: &str) -> String {
    let trimmed = prefix.trim_matches('/').trim();
    if trimmed.is_empty() {
        "/admin".to_string()
    } else {
        format!("/{trimmed}")
    }
}

#[cfg(test)]
mod tests;
