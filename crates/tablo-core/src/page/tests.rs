use topcoat::{Result, context::Cx, view::View};

use super::{MountedPage, Page, PageDef};
use crate::navigation::NavigationItem;

struct MediaLibraryPage;
impl Page for MediaLibraryPage {
    async fn render(_cx: &Cx) -> Result<impl View> {
        Ok(())
    }
}

#[test]
fn defaults_derive_from_the_type_name() {
    let page = MountedPage::new::<MediaLibraryPage>(PageDef::new());
    assert_eq!(page.slug, "media-library");
    assert_eq!(page.navigation.label, "Media library");
    assert_eq!(page.navigation.url(), None);
}

/// The def's order and group apply over a replaced entry, as a resource's do.
#[test]
fn navigation_settings_apply_over_a_replaced_entry() {
    let page = MountedPage::new::<MediaLibraryPage>(
        PageDef::new()
            .label("Files")
            .navigation(NavigationItem::at("Uploads", "/elsewhere"))
            .navigation_order(3)
            .navigation_group("Content"),
    );
    assert_eq!(page.navigation.label, "Uploads");
    assert_eq!(page.navigation.url(), Some("/elsewhere"));
    assert_eq!(page.navigation.order, 3);
    assert_eq!(page.navigation.group.as_deref(), Some("Content"));
}
