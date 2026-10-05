//! [`Page`] — a panel page that is not a resource: a dashboard, a report, a
//! settings screen, a media library.

use std::future::Future;

use topcoat::{Result, context::Cx, view::View};

use crate::resource::{
    NavigationItem,
    naming::{kebab_case, sentence_case, type_stem},
};

/// A panel page that is not a resource, mounted at `{prefix}/{slug}` with a
/// sidebar entry; the panel owns the route and entry while the page owns its
/// markup.
///
/// ```text
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
/// A page serves one `GET` with a sidebar entry; a form it renders posts to an
/// app-declared route the auth gate covers.
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

    /// The sidebar entry; override to set the `order` or the icon.
    fn navigation() -> NavigationItem {
        NavigationItem::for_page::<Self>()
    }

    /// Render the page body. The panel checks for a resolved user before
    /// calling it.
    fn render(cx: &Cx) -> impl Future<Output = Result<impl View>> + Send;
}

#[cfg(test)]
mod tests;
