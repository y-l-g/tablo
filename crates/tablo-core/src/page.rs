//! [`Page`] — a panel page that is not a resource: a dashboard, a report, a
//! settings screen, a media library.

use std::future::Future;

use topcoat::{
    Result,
    context::Cx,
    view::{BoxView, View, ViewExt},
};

use crate::{
    HeaderActions,
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
/// A page serves one `GET` with a sidebar entry; a form it renders posts to an
/// app-declared route the auth gate covers. [`can_access`](Page::can_access) limits both to
/// the users it admits.
pub trait Page: Sized + Send + Sync + 'static {
    /// The URL segment under the panel prefix. Default: the type name without
    /// a `Page` suffix, kebab-cased (`MediaLibraryPage` → `media-library`).
    ///
    /// [`Panel::home`](crate::Panel::home) mounts the page at the prefix and
    /// does not read it.
    fn slug() -> String {
        kebab_case(type_stem::<Self>("Page"))
    }

    /// The sidebar label. Default: the type name without a `Page` suffix, in
    /// sentence case (`MediaLibraryPage` → `Media library`).
    fn navigation_label() -> String {
        sentence_case(type_stem::<Self>("Page"))
    }

    /// The sidebar entry; override to set the `order`, the icon or the group.
    fn navigation() -> NavigationItem {
        NavigationItem::for_page::<Self>()
    }

    /// Whether the request's user may open this page. The panel answers 403 and leaves the page
    /// out of the sidebar when it answers `false`. Default: every signed-in user.
    ///
    /// ```rust
    /// # use tablo_core::{Page, auth};
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
    ///     fn can_access(cx: &Cx) -> bool {
    ///         auth::user::<Staff>(cx).is_some_and(|staff| staff.admin)
    ///     }
    ///
    ///     async fn render(cx: &Cx) -> Result<impl View> {
    ///         Ok(view! { cx => tablo_ui::page(tablo_ui::page_title("Billing")) })
    ///     }
    /// }
    /// ```
    fn can_access(cx: &Cx) -> bool {
        let _ = cx;
        true
    }

    /// The [`HeaderAction`](crate::HeaderAction)s the page serves, each at
    /// `{url}/-/actions/{NAME}`. [`header_action_buttons`](crate::header_action_buttons) renders
    /// their buttons where [`render`](Page::render) places it. Default: none.
    ///
    /// The actions run for whoever may open the page: a page has no policy. An action that writes
    /// a resource's records asks that resource's policy in its own
    /// [`can_run`](crate::HeaderAction::can_run).
    ///
    /// The panel calls it as it mounts, and again on each request that renders or runs one.
    fn header_actions() -> HeaderActions {
        HeaderActions::new()
    }

    /// Render the page body. The panel checks for a resolved user and
    /// [`can_access`](Page::can_access) before calling it.
    fn render(cx: &Cx) -> impl Future<Output = Result<impl View>> + Send;
}

impl NavigationItem {
    /// The default sidebar entry for the [`Page`] `P`.
    pub fn for_page<P: Page>() -> Self {
        Self {
            label: P::navigation_label(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests;

/// The header buttons of the page `P` the request may run, for its
/// [`page_actions`](tablo_ui::page_actions):
///
/// ```rust
/// # use tablo_core::{HeaderAction, HeaderActions, Page};
/// # use topcoat::{Result, context::Cx, view::{View, view}};
/// # struct ClearCache;
/// # impl HeaderAction for ClearCache {
/// #     type Input = ();
/// #     const NAME: &'static str = "clear-cache";
/// #     async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> Result<()> { Ok(()) }
/// # }
/// struct MaintenancePage;
///
/// impl Page for MaintenancePage {
///     fn header_actions() -> HeaderActions {
///         HeaderActions::new().add::<ClearCache>()
///     }
///
///     async fn render(cx: &Cx) -> Result<impl View> {
///         Ok(view! {
///             cx =>
///             tablo_ui::page(
///                 tablo_ui::page_header(
///                     tablo_ui::page_title("Maintenance")
///                     tablo_ui::page_actions((tablo_core::header_action_buttons::<Self>(cx)))
///                 )
///             )
///         })
///     }
/// }
/// ```
///
/// It renders nothing outside a request to `P`'s panel or for a request
/// [`can_access`](Page::can_access) refuses, and no button for an action whose
/// [`can_run`](crate::HeaderAction::can_run) refuses the request.
pub fn header_action_buttons<P: Page>(cx: &Cx) -> BoxView<'_> {
    let Some(url) = crate::panel::url::page::<P>(cx).filter(|_| P::can_access(cx)) else {
        return ().boxed();
    };
    crate::panel::header_bar(cx, &url, &P::header_actions(), |_| true).render(cx)
}
