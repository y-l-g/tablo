use topcoat::{Result, context::Cx, view::View};

use super::Page;

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
    assert_eq!(MediaLibraryPage::navigation_label(), "Media library");
    assert_eq!(Dashboard::slug(), "dashboard");
    assert_eq!(Dashboard::navigation().label, "Dashboard");
    assert_eq!(Dashboard::navigation().url(), None);
}
