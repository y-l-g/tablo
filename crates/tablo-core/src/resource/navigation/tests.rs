use toasty::stmt::List;
use topcoat::context::Cx;

use super::*;
use crate::test_support::User;

struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = crate::NoForm<Self::Model>;

    fn table(_cx: &Cx) -> crate::resource::Table<User> {
        crate::resource::Table::new(
            |r: &User| r.id.to_string(),
            crate::resource::TextColumn::r#for(User::fields().name(), |r: &User| r.name.clone()),
        )
    }

    fn query(_cx: &Cx) -> toasty::stmt::Query<List<User>> {
        // Custom scoping example: only users named Ada
        toasty::stmt::Query::<List<User>>::all().filter(User::fields().name().eq("Ada"))
    }
}

#[test]
fn for_resource_derives_label_only() {
    // The default `Resource::navigation` entry: label from the pluralized
    // model name, and no URL at all — the owning Panel supplies it, so a
    // resource can never link at a mount it guessed.
    let item = NavigationItem::for_resource::<UserResource>();
    assert_eq!(item.label, "Users");
    assert!(matches!(item.target, NavTarget::Derived));
    assert_eq!(item.url(), None);
    assert_eq!(item.order, 0);
    // Unresolved, it is current nowhere.
    assert!(!item.is_current_path("/admin/users"));
}

/// GH #165: `Derived` is resolved by the owning Panel from its own prefix
/// plus the resource's slug — exactly once — while an explicit URL is the
/// author's, even one shaped like another panel's mount.
#[test]
fn derived_targets_resolve_against_the_owning_panel() {
    let derived = NavigationItem::for_resource::<UserResource>();

    // Non-`/admin` panel → this panel's mount, never `/admin/users`.
    let resolved = derived.clone().resolved("/backoffice", "users");
    assert_eq!(resolved.url(), Some("/backoffice/users"));
    assert_eq!(resolved.label, "Users");
    assert_eq!(
        NavigationItem {
            order: -1,
            ..derived.clone()
        }
        .resolved("/backoffice", "users")
        .order,
        -1
    );
    // Already resolved: resolving again is a no-op, however the second
    // panel is mounted.
    assert_eq!(
        resolved.clone().resolved("elsewhere", "users").url(),
        Some("/backoffice/users")
    );

    // Mount normalisation follows `Panel::new`.
    assert_eq!(
        derived.clone().resolved("/admin", "users").url(),
        Some("/admin/users")
    );
    assert_eq!(
        derived.clone().resolved("/backoffice/", "users").url(),
        Some("/backoffice/users")
    );
    assert_eq!(
        derived.clone().resolved("", "users").url(),
        Some("/admin/users")
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
            spelled_out.resolved("backoffice", "users").url(),
            Some(url),
            "explicit URL must survive resolution"
        );
    }
}

#[test]
fn slugs_follow_the_filament_convention() {
    // UserResource → strip "Resource" → pluralize → kebab-case
    assert_eq!(<UserResource as Resource>::slug(), "users");
    assert_eq!(UserResource::navigation_label(), "Users");
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
