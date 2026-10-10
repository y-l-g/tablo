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

/// A replaced entry keeps its own label and URL, and the def's icon and order
/// still apply to it.
#[test]
fn replaced_navigation_item_takes_the_def_icon_order_and_group() {
    struct DraftsResource;
    impl Resource for DraftsResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .icon(tablo_ui::icons::INFO)
                .navigation_order(3)
                .navigation_group("Content")
                .navigation(NavigationItem::at("Drafts", "/admin/dummies?f.draft=1"))
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }

    let item = nav_item::<DraftsResource>(&Panel::new("admin"));
    assert_eq!(item.label, "Drafts");
    assert_eq!(item.url(), Some("/admin/dummies?f.draft=1"));
    assert_eq!(item.order, 3);
    assert!(item.icon.is_some());
    assert_eq!(item.group.as_deref(), Some("Content"));
}

#[test]
fn panel_normalizes_prefix() {
    assert_eq!(Panel::new("admin").prefix(), "/admin");
    assert_eq!(Panel::new("/admin").prefix(), "/admin");
    assert_eq!(Panel::new("admin/").prefix(), "/admin");
    assert_eq!(Panel::new("/admin/").prefix(), "/admin");
    assert_eq!(Panel::new("").prefix(), "/admin");
}
