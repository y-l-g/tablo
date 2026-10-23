use toasty::stmt::List;
use topcoat::context::Cx;

use super::*;
use crate::{Resource, ResourceDef, lens, test_support::User};

struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new().table(crate::table::Table::new(crate::TextColumn::new(lens!(
            User.name
        ))))
    }

    fn query(_cx: &Cx) -> toasty::stmt::Query<List<User>> {
        // Custom scoping example: only users named Ada
        toasty::stmt::Query::<List<User>>::all().filter(User::fields().name().eq("Ada"))
    }
}

/// `Derived` is resolved to the URL the owning Panel mounts it at —
/// exactly once — while an explicit URL is the author's, even one shaped like
/// another panel's mount.
#[test]
fn derived_targets_resolve_against_the_owning_panel() {
    let derived = NavigationItem {
        label: "Users".into(),
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

    // Explicit URLs survive verbatim, including `/admin/users`, which matches a derived default.
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
    let mounted = crate::resource::Mounted::new(UserResource::declare(), "/admin");
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
    assert!(users.is_current_path("/admin/users/-/create"));
    assert!(showcase.is_current_path("/admin/showcase/table"));
    // slash-boundary — near-misses inactive
    assert!(!users.is_current_path("/admin/userships"));
    assert!(!showcase.is_current_path("/admin/showcases"));
    assert!(!showcase.is_current_path("/admin/showcase-table"));
    // unrelated
    assert!(!users.is_current_path("/other"));
    assert!(!showcase.is_current_path("/admin/users"));
    // an entry's query or fragment is the state it points with, not part of its path
    let posts = NavigationItem::at("Posts", "/admin/posts?f.status=draft");
    assert!(posts.is_current_path("/admin/posts"));
    assert!(posts.is_current_path("/admin/posts/-/create"));
    assert!(!posts.is_current_path("/admin/postscripts"));
    let section = NavigationItem::at("Section", "/admin/posts#top");
    assert!(section.is_current_path("/admin/posts"));
    // an entry naming no path is current nowhere
    let query_only = NavigationItem::at("Query", "?f.status=draft");
    assert!(!query_only.is_current_path("/admin/posts"));
}

/// Of the entries holding a path, the longest path wins; of those sharing it, the entry whose
/// query the request carries wins, then the one naming more of it, then the first declared.
#[test]
fn current_index_prefers_the_entry_the_request_holds() {
    let items = [
        NavigationItem::at("Drafts", "/admin/users?f.status=draft"),
        NavigationItem::at("Users", "/admin/users"),
        NavigationItem::at("Dashboard", "/admin"),
        NavigationItem::at("Both", "/admin/users?f.status=draft&sort=name"),
    ];
    assert_eq!(
        current_index(&items, "/admin/users", ""),
        Some(1),
        "the plain entry, not the one its query names"
    );
    assert_eq!(
        current_index(&items, "/admin/users", "f.status=review"),
        Some(1)
    );
    assert_eq!(
        current_index(&items, "/admin/users", "f.status=draft"),
        Some(0)
    );
    assert_eq!(
        current_index(&items, "/admin/users", "sort=name&f.status=draft"),
        Some(3),
        "the entry naming more of the request's query"
    );
    assert_eq!(current_index(&items, "/admin/media", ""), Some(2));
    assert_eq!(current_index(&items, "/elsewhere", ""), None);
}
