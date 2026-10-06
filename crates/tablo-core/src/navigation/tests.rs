use toasty::stmt::List;
use topcoat::context::Cx;

use super::*;
use crate::{Resource, ResourceDef, lens, test_support::User};

struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().table(crate::table::Table::new(crate::table::TextColumn::new(
            lens!(User.name),
        )))
    }

    fn query(_cx: &Cx) -> toasty::stmt::Query<List<User>> {
        // Custom scoping example: only users named Ada
        toasty::stmt::Query::<List<User>>::all().filter(User::fields().name().eq("Ada"))
    }
}

#[test]
fn the_default_entry_is_the_plural_label_at_the_list() {
    // A def that sets no navigation: label from the pluralized model name, URL from the panel
    // that mounts it.
    let item = crate::resource::Mounted::new(
        UserResource::declare(),
        "/backoffice",
        &crate::schema::FieldResolver::default(),
    )
    .navigation;
    assert_eq!(item.label, "Users");
    assert_eq!(item.url(), Some("/backoffice/users"));
    assert_eq!(item.order, 0);
    assert!(item.icon.is_none());
}

/// `Derived` is resolved to the URL the owning Panel mounts it at —
/// exactly once — while an explicit URL is the author's, even one shaped like
/// another panel's mount.
#[test]
fn derived_targets_resolve_against_the_owning_panel() {
    let derived = NavigationItem {
        label: "Users".to_string(),
        ..NavigationItem::default()
    };

    // Non-`/admin` panel → this panel's mount, never `/admin/users`.
    let resolved = derived.clone().resolved("/backoffice/users");
    assert_eq!(resolved.url(), Some("/backoffice/users"));
    assert_eq!(resolved.label, "Users");
    assert_eq!(
        NavigationItem {
            order: -1,
            ..derived.clone()
        }
        .resolved("/backoffice/users")
        .order,
        -1
    );
    // Already resolved: resolving again is a no-op, however the second
    // panel is mounted.
    assert_eq!(
        resolved.clone().resolved("/elsewhere/users").url(),
        Some("/backoffice/users")
    );

    // Explicit URLs survive verbatim — including `/admin/users`, which the
    // old origin-mount heuristic could not tell from a derived default.
    for url in [
        "/admin/users",
        "/admin/posts?f.status=draft",
        "/backoffice/users/drafts",
        "/reports/users",
    ] {
        let spelled_out = NavigationItem::at("Users", url);
        assert_eq!(
            spelled_out.resolved("/backoffice/users").url(),
            Some(url),
            "explicit URL must survive resolution"
        );
    }
}

#[test]
fn slugs_follow_the_filament_convention() {
    // UserResource → strip "Resource" → pluralize → kebab-case
    let mounted = crate::resource::Mounted::new(
        UserResource::declare(),
        "/admin",
        &crate::schema::FieldResolver::default(),
    );
    assert_eq!(mounted.slug, "users");
    assert_eq!(mounted.plural_label, "Users");
}

#[test]
fn navigation_item_is_current_path() {
    let users = NavigationItem::at("Users", "/admin/users");
    let showcase = NavigationItem::at("Showcase", "/admin/showcase");
    // exact
    assert!(users.is_current_path("/admin/users"));
    assert!(showcase.is_current_path("/admin/showcase"));
    // slash-boundary — sub-pages active
    assert!(users.is_current_path("/admin/users/create"));
    assert!(showcase.is_current_path("/admin/showcase/table"));
    // slash-boundary — near-misses inactive
    assert!(!users.is_current_path("/admin/userships"));
    assert!(!showcase.is_current_path("/admin/showcases"));
    assert!(!showcase.is_current_path("/admin/showcase-table"));
    // unrelated
    assert!(!users.is_current_path("/other"));
    assert!(!showcase.is_current_path("/admin/users"));
}
