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

/// A URL an override spells out is the author's, not the panel's —
/// only a `Derived` target is resolved. A cross-panel link, a query view, or
/// a custom path segment must survive untouched on a non-`/admin` panel,
/// *including* one that looks like the origin mount.
#[test]
fn panel_navigation_item_keeps_urls_the_override_spells_out() {
    use crate::navigation::NavTarget;

    struct DraftsResource;
    impl Resource for DraftsResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            // Order only, no URL: still the panel's to resolve.
            ResourceDef::new()
                .slug("drafts")
                .plural_label("Drafts")
                .navigation_order(3)
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }

    // `label`/`order` decorate the default item without touching its URL,
    // so the panel still owns (and resolves) the URL.
    let decorated = nav_item::<DraftsResource>(&Panel::new("backoffice"));
    assert_eq!(decorated.label, "Drafts");
    assert_eq!(decorated.order, 3);
    assert_eq!(decorated.url(), Some("/backoffice/drafts"));

    // A spelled-out URL is left alone — `/admin/posts?…` on a `/backoffice`
    // panel is a deliberate link, not a stale mount.
    struct ReportsResource;
    impl Resource for ReportsResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("reports")
                .navigation(NavigationItem::at(
                    "Draft posts",
                    "/admin/posts?f.status=draft",
                ))
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }
    let spelled_out = nav_item::<ReportsResource>(&Panel::new("backoffice"));
    assert_eq!(spelled_out.url(), Some("/admin/posts?f.status=draft"));
    assert_eq!(spelled_out.label, "Draft posts");
    assert!(matches!(spelled_out.target, NavTarget::Url(_)));

    // The same URL spelled out on the resource's *own* slug is the author's
    // too: `Derived` is what the Panel resolves, never a URL that happens to
    // match the origin mount.
    struct OwnSlugResource;
    impl Resource for OwnSlugResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("users")
                .navigation(NavigationItem::at("Users (legacy)", "/admin/users"))
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }
    let own_slug = nav_item::<OwnSlugResource>(&Panel::new("backoffice"));
    assert_eq!(own_slug.url(), Some("/admin/users"));
}

#[test]
fn panel_navigation_item_respects_prefix() {
    struct DummyResource;
    impl Resource for DummyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Dummy.name),
            )))
        }
    }

    let panel = Panel::new("backoffice");
    let item = nav_item::<DummyResource>(&panel);
    // Label: pluralized model name ("Dummy" → "Dummies"); URL: prefix +
    // resource slug ("DummyResource" → "dummies"), resolved by the panel.
    assert_eq!(item.label, "Dummies");
    assert_eq!(item.url(), Some("/backoffice/dummies"));

    let default = nav_item::<DummyResource>(&Panel::new("admin"));
    assert_eq!(default.url(), Some("/admin/dummies"));
    // Mount normalisation is `Panel::new`'s (slashes trimmed, `/admin` when
    // empty), and the resolved URL follows it.
    let slashed = nav_item::<DummyResource>(&Panel::new("/backoffice/"));
    assert_eq!(slashed.url(), Some("/backoffice/dummies"));
    let bare = nav_item::<DummyResource>(&Panel::new(""));
    assert_eq!(bare.url(), Some("/admin/dummies"));
}

#[test]
fn panel_navigation_items_are_distinct_for_multiple_resources() {
    struct UserResource;
    impl Resource for UserResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
                lens!(Dummy.name),
            )))
        }
    }
    struct CategoryResource;
    impl Resource for CategoryResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("categories")
                .plural_label("Categories")
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }

    let panel = Panel::new("admin");
    let users = nav_item::<UserResource>(&panel);
    let categories = nav_item::<CategoryResource>(&panel);
    assert_eq!(users.url(), Some("/admin/users"));
    assert_eq!(categories.url(), Some("/admin/categories"));
    assert_ne!(users.url(), categories.url());
}

#[test]
fn panel_normalizes_prefix() {
    assert_eq!(Panel::new("admin").prefix(), "/admin");
    assert_eq!(Panel::new("/admin").prefix(), "/admin");
    assert_eq!(Panel::new("admin/").prefix(), "/admin");
    assert_eq!(Panel::new("/admin/").prefix(), "/admin");
    assert_eq!(Panel::new("").prefix(), "/admin");
}

/// The override reaches *rendered* sidebar order.
/// Rendered on a non-`/admin` panel, so the same test also pins the URL
/// half: the sidebar links under `/backoffice`, never the origin `/admin`.
#[tokio::test]
async fn panel_sidebar_renders_overridden_navigation_order_first() {
    use topcoat::{
        context::CxTestBuilder,
        view::{ViewExt, view},
    };

    struct PinnedResource;
    impl Resource for PinnedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            // A def that only sets the order.
            ResourceDef::new()
                .slug("pinned")
                .navigation_order(-1)
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }
    struct OtherResource;
    impl Resource for OtherResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("other")
                .plural_label("Other")
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
        }
    }

    // `PinnedResource` is declared last, so only the override can move it up.
    let panel = Panel::new("backoffice");
    let nav_items = vec![
        nav_item::<OtherResource>(&panel),
        nav_item::<PinnedResource>(&panel),
    ];
    let (parts, ()) = http::Request::builder()
        .uri("/backoffice/other")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    let cx_ref = &cx;
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &nav_items, "/backoffice/other", slot, None)
        .await
        .unwrap()
        .single()
        .await
        .unwrap()
        .render(&cx);
    let pinned_at = html
        .find("/backoffice/pinned")
        .unwrap_or_else(|| panic!("pinned item must link under the panel prefix, got {html}"));
    let other_at = html.find("/backoffice/other").expect("other item renders");
    assert!(
        !html.contains("/admin/pinned"),
        "navigation must not link at the origin mount, got {html}"
    );
    assert!(
        pinned_at < other_at,
        "an overridden order: -1 must render first, got {html}"
    );
}
