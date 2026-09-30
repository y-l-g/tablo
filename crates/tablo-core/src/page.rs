//! [`Page`] — a panel page that is not a resource: a dashboard, a report, a
//! settings screen, a media library.

use std::future::Future;

use topcoat::{Result, context::Cx, view::View};

use crate::resource::{
    NavigationItem,
    naming::{kebab_case, sentence_case, type_stem},
};

/// A page the [`Panel`](crate::Panel) mounts and lists in its sidebar.
///
/// Registered with [`Panel::page`](crate::Panel::page) at `{prefix}/{slug}`,
/// or with [`Panel::home`](crate::Panel::home) at the panel prefix itself.
/// The panel owns the route and the sidebar entry, the way it owns a
/// [`Resource`](crate::Resource)'s; the page owns its markup. The app's
/// `#[layout]` at the panel prefix frames it in the shell, as it frames every
/// panel page.
///
/// ```ignore
/// struct ReportsPage;
///
/// impl Page for ReportsPage {
///     async fn render(cx: &Cx) -> Result<impl View> {
///         Ok(view! { cx => tablo_ui::page(tablo_ui::page_title("Reports")) })
///     }
/// }
///
/// Panel::new("admin").page::<ReportsPage>() // GET /admin/reports
/// ```
///
/// A page serves one `GET` and always has a sidebar entry. A form it renders
/// posts to a route the app declares with `#[route]`; under the panel prefix,
/// the auth gate covers it.
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

    /// The sidebar entry. Override it to set the `order` or the icon; the panel resolves
    /// its URL, as it does for [`Resource::navigation`](crate::Resource::navigation).
    fn navigation() -> NavigationItem {
        NavigationItem::for_page::<Self>()
    }

    /// Render the page body. The panel checks for a resolved user before
    /// calling it.
    fn render(cx: &Cx) -> impl Future<Output = Result<impl View>> + Send;
}

#[cfg(test)]
mod tests;
