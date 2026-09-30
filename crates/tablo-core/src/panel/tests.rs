use super::*;
use crate::panel::test_support::Dummy;

/// GH #165: `Resource::navigation()` reaches the sidebar, and its order is
/// what the rendered shell sorts by.
#[test]
fn panel_navigation_item_honours_override_order_with_prefix_adjusted_url() {
    use crate::resource::{NavigationItem, Resource};

    struct DummyResource;
    impl Resource for DummyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &topcoat::context::Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }

        fn navigation() -> NavigationItem {
            // The override cannot know the panel prefix, so it decorates
            // the default item: order here, URL from the panel.
            NavigationItem {
                order: -1,
                ..NavigationItem::for_resource::<Self>()
            }
        }
    }
    struct PlainResource;
    impl Resource for PlainResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &topcoat::context::Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }

        fn slug() -> String {
            "plain".to_string()
        }
    }

    // Non-`/admin` panel + overridden navigation: the order survives and
    // the URL is resolved under this panel's prefix, not `/admin`.
    let panel = Panel::new("backoffice");
    let item = panel.nav_item::<DummyResource>();
    assert_eq!(item.order, -1);
    assert_eq!(item.label, "Dummies");
    assert_eq!(item.url(), Some("/backoffice/dummies"));
    // A resource without an override keeps the default (declaration order).
    assert_eq!(panel.nav_item::<PlainResource>().order, 0);
}

/// GH #165: a URL an override spells out is the author's, not the panel's —
/// only a `Derived` target is resolved. A cross-panel link, a query view, or
/// a custom path segment must survive untouched on a non-`/admin` panel,
/// *including* one that looks like the origin mount.
#[test]
fn panel_navigation_item_keeps_urls_the_override_spells_out() {
    use crate::resource::{NavTarget, NavigationItem, Resource};

    struct DraftsResource;
    impl Resource for DraftsResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &topcoat::context::Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }

        fn slug() -> String {
            "drafts".to_string()
        }

        fn navigation_label() -> String {
            "Drafts".to_string()
        }

        fn navigation() -> NavigationItem {
            // Order only, no URL: still the panel's to resolve.
            NavigationItem {
                order: 3,
                ..NavigationItem::for_resource::<Self>()
            }
        }
    }

    // `label`/`order` decorate the default item without touching its URL,
    // so the panel still owns (and resolves) the URL.
    let decorated = Panel::new("backoffice").nav_item::<DraftsResource>();
    assert_eq!(decorated.label, "Drafts");
    assert_eq!(decorated.order, 3);
    assert_eq!(decorated.url(), Some("/backoffice/drafts"));

    // A spelled-out URL is left alone — `/admin/posts?…` on a `/backoffice`
    // panel is a deliberate link, not a stale mount.
    struct ReportsResource;
    impl Resource for ReportsResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &topcoat::context::Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }

        fn slug() -> String {
            "reports".to_string()
        }

        fn navigation() -> NavigationItem {
            NavigationItem::at("Draft posts", "/admin/posts?filters=status:draft")
        }
    }
    let spelled_out = Panel::new("backoffice").nav_item::<ReportsResource>();
    assert_eq!(spelled_out.url(), Some("/admin/posts?filters=status:draft"));
    assert_eq!(spelled_out.label, "Draft posts");
    assert!(matches!(spelled_out.target, NavTarget::Url(_)));

    // The same URL spelled out on the resource's *own* slug is the author's
    // too: `Derived` is what the Panel resolves, never a URL that happens to
    // match the origin mount.
    struct OwnSlugResource;
    impl Resource for OwnSlugResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &topcoat::context::Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }

        fn slug() -> String {
            "users".to_string()
        }

        fn navigation() -> NavigationItem {
            NavigationItem::at("Users (legacy)", "/admin/users")
        }
    }
    let own_slug = Panel::new("backoffice").nav_item::<OwnSlugResource>();
    assert_eq!(own_slug.url(), Some("/admin/users"));
}

#[test]
fn panel_navigation_item_respects_prefix() {
    use crate::resource::Resource;

    struct DummyResource;
    impl Resource for DummyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &topcoat::context::Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }
    }

    let panel = Panel::new("backoffice");
    let item = panel.nav_item::<DummyResource>();
    // Label: pluralized model name ("Dummy" → "Dummies"); URL: prefix +
    // resource slug ("DummyResource" → "dummies"), resolved by the panel.
    assert_eq!(item.label, "Dummies");
    assert_eq!(item.url(), Some("/backoffice/dummies"));

    let default = Panel::new("admin").nav_item::<DummyResource>();
    assert_eq!(default.url(), Some("/admin/dummies"));
    // Mount normalisation is `Panel::new`'s (slashes trimmed, `/admin` when
    // empty), and the resolved URL follows it.
    let slashed = Panel::new("/backoffice/").nav_item::<DummyResource>();
    assert_eq!(slashed.url(), Some("/backoffice/dummies"));
    let bare = Panel::new("").nav_item::<DummyResource>();
    assert_eq!(bare.url(), Some("/admin/dummies"));
}

#[test]
fn panel_navigation_items_are_distinct_for_multiple_resources() {
    use crate::resource::Resource;

    struct UserResource;
    impl Resource for UserResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &topcoat::context::Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }
    }
    struct CategoryResource;
    impl Resource for CategoryResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &topcoat::context::Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }

        fn slug() -> String {
            "categories".to_string()
        }

        fn navigation_label() -> String {
            "Categories".to_string()
        }
    }

    let panel = Panel::new("admin");
    let users = panel.nav_item::<UserResource>();
    let categories = panel.nav_item::<CategoryResource>();
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

/// GH #165: the override reaches *rendered* sidebar order.
/// Rendered on a non-`/admin` panel, so the same test also pins the URL
/// half: the sidebar links under `/backoffice`, never the origin `/admin`.
#[tokio::test]
async fn panel_sidebar_renders_overridden_navigation_order_first() {
    use topcoat::{
        context::CxTestBuilder,
        view::{ViewExt, view},
    };

    use crate::resource::{NavigationItem, Resource};

    struct PinnedResource;
    impl Resource for PinnedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &topcoat::context::Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }

        fn slug() -> String {
            "pinned".to_string()
        }

        fn navigation() -> NavigationItem {
            // GH #165 regression shape: an override that only sets order.
            // Before the fix the sidebar kept declaration order and the
            // resource's `order: -1` had no effect at all.
            NavigationItem {
                order: -1,
                ..NavigationItem::for_resource::<Self>()
            }
        }
    }
    struct OtherResource;
    impl Resource for OtherResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn table(_cx: &topcoat::context::Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new(
                |r: &Dummy| r.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |r: &Dummy| {
                    r.name.clone()
                }),
            )
        }

        fn slug() -> String {
            "other".to_string()
        }

        fn navigation_label() -> String {
            "Other".to_string()
        }
    }

    // `PinnedResource` is declared last, so only the override can move it up.
    let panel = Panel::new("backoffice");
    let nav_items = vec![
        panel.nav_item::<OtherResource>(),
        panel.nav_item::<PinnedResource>(),
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
