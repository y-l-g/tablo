//! [`Page`] — a panel page that is not a resource: a dashboard, a report, a
//! settings screen, a media library — and the [`PageDef`] it declares.

use std::{future::Future, sync::Arc};

use topcoat::{Result, context::Cx, icon::IconData, view::View};

use crate::{
    HeaderAction, HeaderActions,
    naming::{kebab_case, sentence_case, type_stem},
    navigation::NavigationItem,
};

/// A panel page that is not a resource, mounted at `{prefix}/{slug}` with a
/// sidebar entry; the panel owns the route and entry while the page owns its
/// markup.
///
/// ```rust
/// # use tablo_core::{Page, Panel};
/// # use topcoat::{Result, context::Cx, view::{View, view}};
/// struct ReportsPage;
///
/// impl Page for ReportsPage {
///     async fn render(cx: &Cx) -> Result<impl View> {
///         Ok(view! { cx => tablo_ui::page(tablo_ui::page_title("Reports")) })
///     }
/// }
///
/// Panel::new("admin").page::<ReportsPage>(); // GET /admin/reports
/// ```
///
/// A page serves one `GET` with a sidebar entry, which [`PageDef::access`] limits
/// to the users it admits. A form it renders posts to an app-declared route the
/// auth gate covers; that route asks [`can_open`](crate::can_open) itself.
pub trait Page: Sized + Send + Sync + 'static {
    /// The page's slug, sidebar entry, access and header actions. Defaults to
    /// [`PageDef::new`]: every default.
    ///
    /// [`Panel::page_with`](crate::Panel::page_with) adjusts it for one panel.
    fn declare() -> PageDef {
        PageDef::new()
    }

    /// Render the page body. The panel checks for a resolved user and
    /// [`PageDef::access`] before calling it.
    fn render(cx: &Cx) -> impl Future<Output = Result<impl View>> + Send;
}

/// Whether the request's user may open a page.
type AccessFn = dyn Fn(&Cx) -> bool + Send + Sync;

/// What a [`Page`] declares: its slug, sidebar entry, access and header actions.
///
/// [`Page::declare`] returns one, and [`Panel::page_with`](crate::Panel::page_with) adjusts it for
/// one panel. Every setting has a default, so a page sets only what differs:
///
/// ```rust
/// # use tablo_core::{Page, PageDef};
/// # use topcoat::{Result, context::Cx, view::View};
/// struct MediaLibraryPage;
///
/// impl Page for MediaLibraryPage {
///     fn declare() -> PageDef {
///         PageDef::new().slug("media").icon(tablo_ui::icons::IMAGE)
///     }
///
///     async fn render(_cx: &Cx) -> Result<impl View> {
///         Ok(())
///     }
/// }
/// ```
///
/// The panel builds the def once when it mounts and serves the result to every request.
pub struct PageDef {
    slug: Option<String>,
    label: Option<String>,
    icon: Option<IconData>,
    navigation_order: Option<i32>,
    navigation_group: Option<String>,
    navigation: Option<NavigationItem>,
    access: Arc<AccessFn>,
    header_actions: HeaderActions,
}

impl PageDef {
    /// A def with every default: the slug and label from the page's type name, and access for
    /// every signed-in user.
    pub fn new() -> Self {
        Self::default()
    }

    /// The URL segment under the panel prefix: `"media"` mounts the page at
    /// `{panel prefix}/media`.
    ///
    /// Defaults to the page type's name without a trailing `Page`, kebab-cased:
    /// `MediaLibraryPage` → `media-library`. [`Panel::home`](crate::Panel::home) mounts the page
    /// at the prefix and does not read it.
    #[must_use]
    pub fn slug(mut self, slug: impl Into<String>) -> Self {
        self.slug = Some(slug.into());
        self
    }

    /// The sidebar label.
    ///
    /// Defaults to the page type's name without a trailing `Page`, in sentence case:
    /// `MediaLibraryPage` → `Media library`.
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The icon of the page's sidebar entry, replacing the icon of a
    /// [`navigation`](Self::navigation) item.
    #[must_use]
    pub fn icon(mut self, icon: IconData) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Where the sidebar entry sorts among the panel's entries, lowest first; defaults to `0`, and
    /// entries of equal order keep their registration order. It replaces the order of a
    /// [`navigation`](Self::navigation) item.
    #[must_use]
    pub fn navigation_order(mut self, order: i32) -> Self {
        self.navigation_order = Some(order);
        self
    }

    /// The labelled sidebar group the entry renders in, such as `"Reports"`; defaults to none,
    /// above every group. It replaces the group of a [`navigation`](Self::navigation) item.
    #[must_use]
    pub fn navigation_group(mut self, group: impl Into<String>) -> Self {
        self.navigation_group = Some(group.into());
        self
    }

    /// Replaces the sidebar entry, for one that links somewhere other than the page:
    /// `NavigationItem::at("Exports", "/admin/reports?tab=exports")`. [`icon`](Self::icon),
    /// [`navigation_order`](Self::navigation_order) and
    /// [`navigation_group`](Self::navigation_group) still apply to it.
    #[must_use]
    pub fn navigation(mut self, item: NavigationItem) -> Self {
        self.navigation = Some(item);
        self
    }

    /// Whether the request's user may open the page. The panel answers 403 and leaves the page
    /// out of the sidebar when it answers `false`. Defaults to every signed-in user.
    ///
    /// ```rust
    /// # use tablo_core::{Page, PageDef, auth};
    /// # use topcoat::{Result, context::Cx, view::{View, view}};
    /// # #[derive(Clone)]
    /// # struct Staff { admin: bool }
    /// # impl tablo_core::PanelUser for Staff {
    /// #     fn user_id(&self) -> String { String::new() }
    /// #     fn display_name(&self) -> &str { "" }
    /// # }
    /// struct BillingPage;
    ///
    /// impl Page for BillingPage {
    ///     fn declare() -> PageDef {
    ///         PageDef::new().access(|cx| auth::user::<Staff>(cx).is_some_and(|staff| staff.admin))
    ///     }
    ///
    ///     async fn render(cx: &Cx) -> Result<impl View> {
    ///         Ok(view! { cx => tablo_ui::page(tablo_ui::page_title("Billing")) })
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn access(mut self, access: impl Fn(&Cx) -> bool + Send + Sync + 'static) -> Self {
        self.access = Arc::new(access);
        self
    }

    /// Adds the [`HeaderAction`] `A` after the ones already declared, served at
    /// `{url}/-/actions/{NAME}`. [`header_action_buttons`](crate::header_action_buttons) renders
    /// their buttons where [`render`](Page::render) places it. An `A::NAME` that is not a
    /// single URL path segment does not compile.
    ///
    /// The actions run for whoever may open the page: a page has no policy. An action that writes
    /// a resource's records asks that resource's policy in its own
    /// [`can_run`](crate::HeaderAction::can_run).
    #[must_use]
    pub fn header_action<A: HeaderAction>(mut self) -> Self {
        self.header_actions = self.header_actions.add::<A>();
        self
    }
}

impl Default for PageDef {
    fn default() -> Self {
        Self {
            slug: None,
            label: None,
            icon: None,
            navigation_order: None,
            navigation_group: None,
            navigation: None,
            access: Arc::new(|_| true),
            header_actions: HeaderActions::new(),
        }
    }
}

impl std::fmt::Debug for PageDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageDef")
            .field("slug", &self.slug)
            .field("label", &self.label)
            .field("navigation_order", &self.navigation_order)
            .field("navigation_group", &self.navigation_group)
            .field("navigation", &self.navigation)
            .field("header_actions", &self.header_actions)
            .finish_non_exhaustive()
    }
}

/// A [`PageDef`] with its defaults filled in for the page `P`, as a panel mounted it.
pub(crate) struct MountedPage {
    pub(crate) slug: String,
    /// The sidebar entry, its target not yet resolved to the page's URL.
    pub(crate) navigation: NavigationItem,
    access: Arc<AccessFn>,
    pub(crate) header_actions: HeaderActions,
}

impl MountedPage {
    pub(crate) fn new<P: Page>(def: PageDef) -> Self {
        let stem = type_stem::<P>("Page");
        let mut navigation = def.navigation.unwrap_or_else(|| NavigationItem {
            label: def.label.unwrap_or_else(|| sentence_case(stem)),
            ..NavigationItem::default()
        });
        if let Some(order) = def.navigation_order {
            navigation.order = order;
        }
        if let Some(icon) = def.icon {
            navigation.icon = Some(icon);
        }
        if let Some(group) = def.navigation_group {
            navigation.group = Some(group);
        }
        Self {
            slug: def.slug.unwrap_or_else(|| kebab_case(stem)),
            navigation,
            access: def.access,
            header_actions: def.header_actions,
        }
    }

    /// Whether the request's user may open the page.
    pub(crate) fn admits(&self, cx: &Cx) -> bool {
        (self.access)(cx)
    }

    /// The page's access check, shared with its sidebar entry.
    pub(crate) fn access(&self) -> Arc<AccessFn> {
        Arc::clone(&self.access)
    }
}

#[cfg(test)]
mod tests;
