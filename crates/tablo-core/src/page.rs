//! [`Page`] — a panel page that is not a resource: a dashboard, a report, a
//! settings screen, a media library.

use std::future::Future;

use topcoat::{Result, context::Cx, view::View};

use crate::resource::{
    NavigationItem,
    naming::{kebab_case, type_short_name},
};

/// A page the [`Panel`](crate::Panel) mounts and lists in its sidebar.
///
/// Registered with [`Panel::page`](crate::Panel::page) at `{prefix}/{slug}`,
/// or with [`Panel::home`](crate::Panel::home) at the panel prefix itself.
/// The panel owns the route and the sidebar entry, the way it owns a
/// [`Resource`](crate::Resource)'s; the page owns its markup. The panel's
/// `#[layout]` frames it like every other panel page.
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
/// A page serves one `GET`. A form it renders posts to a route the app
/// declares with `#[route]`; under the panel prefix, the auth gate covers it.
pub trait Page: 'static {
    /// The URL segment under the panel prefix. Default: the type name without
    /// a `Page` suffix, kebab-cased (`MediaLibraryPage` → `media-library`).
    ///
    /// [`Panel::home`](crate::Panel::home) mounts the page at the prefix and
    /// does not read it.
    fn slug() -> String {
        kebab_case(page_stem::<Self>())
    }

    /// The sidebar label. Default: the type name without a `Page` suffix, as
    /// words (`MediaLibraryPage` → `Media library`).
    fn label() -> String {
        crate::schema::capitalize(&kebab_case(page_stem::<Self>()).replace('-', " "))
    }

    /// The sidebar entry. Override it to set the `order`; the panel resolves
    /// its URL, as it does for [`Resource::navigation`](crate::Resource::navigation).
    fn navigation() -> NavigationItem {
        NavigationItem::for_page::<Self>()
    }

    /// Render the page body. The panel checks the signed-in user before
    /// calling it.
    fn render(cx: &Cx) -> impl Future<Output = Result<impl View>> + Send;
}

/// The type name a page's defaults derive from, without its `Page` suffix.
fn page_stem<P: ?Sized>() -> &'static str {
    let name = type_short_name::<P>();
    name.strip_suffix("Page")
        .filter(|stem| !stem.is_empty())
        .unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MediaLibraryPage;
    impl Page for MediaLibraryPage {
        async fn render(_cx: &Cx) -> Result<impl View> {
            Ok(())
        }
    }

    struct Dashboard;
    impl Page for Dashboard {
        async fn render(_cx: &Cx) -> Result<impl View> {
            Ok(())
        }
    }

    #[test]
    fn defaults_derive_from_the_type_name() {
        assert_eq!(MediaLibraryPage::slug(), "media-library");
        assert_eq!(MediaLibraryPage::label(), "Media library");
        assert_eq!(Dashboard::slug(), "dashboard");
        assert_eq!(Dashboard::label(), "Dashboard");
        assert_eq!(Dashboard::navigation().label, "Dashboard");
        assert_eq!(Dashboard::navigation().url(), None);
    }
}
