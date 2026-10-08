use super::*;
use crate::{lens, navigation::NavigationItem, panel::test_support::Dummy, resource::Mounted};

/// `R`'s sidebar entry on `panel`.
fn nav_item<R: Resource>(panel: &Panel) -> NavigationItem {
    Mounted::new(
        R::declare(),
        panel.prefix(),
        &crate::schema::FieldResolver::default(),
    )
    .navigation
}

/// The def's navigation reaches the sidebar, and its order is
/// what the rendered shell sorts by.
#[test]
fn panel_navigation_item_honours_override_order_with_prefix_adjusted_url() {
    struct DummyResource;
    impl Resource for DummyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            // The def cannot know the panel prefix, so it sets the order and the panel resolves
            // the URL.
            ResourceDef::new()
                .navigation_order(-1)
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }
    struct PlainResource;
    impl Resource for PlainResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("plain")
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }

    // Non-`/admin` panel + overridden navigation: the order survives and
    // the URL is resolved under this panel's prefix, not `/admin`.
    let panel = Panel::new("backoffice");
    let item = nav_item::<DummyResource>(&panel);
    assert_eq!(item.order, -1);
    assert_eq!(item.label, "Dummies");
    assert_eq!(item.url(), Some("/backoffice/dummies"));
    // A resource without an override keeps the default (declaration order).
    assert_eq!(nav_item::<PlainResource>(&panel).order, 0);
}

#[test]
fn panel_normalizes_prefix() {
    assert_eq!(Panel::new("admin").prefix(), "/admin");
    assert_eq!(Panel::new("/admin").prefix(), "/admin");
    assert_eq!(Panel::new("admin/").prefix(), "/admin");
    assert_eq!(Panel::new("/admin/").prefix(), "/admin");
    assert_eq!(Panel::new("").prefix(), "/admin");
}
