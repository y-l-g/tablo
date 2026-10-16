use super::*;

fn nav() -> Vec<NavigationItem> {
    vec![
        NavigationItem::at("Dashboard", "/admin"),
        NavigationItem::at("Blog Posts", "/admin/posts"),
        NavigationItem::at("Users", "/admin/users?sort=name"),
    ]
}

/// The resources' lists among [`nav`]'s entries.
const LISTS: [&str; 2] = ["/admin/posts", "/admin/users"];

fn labels(crumbs: &[Crumb]) -> Vec<(&str, Option<&str>)> {
    crumbs
        .iter()
        .map(|crumb| (crumb.label.as_str(), crumb.href.as_deref()))
        .collect()
}

/// The longest entry holding the path names the trail, not the home entry every path sits under.
#[test]
fn the_longest_entry_names_the_trail() {
    assert_eq!(
        labels(&trail(&nav(), &LISTS, "/admin/posts", "")),
        [("Blog Posts", None)]
    );
    assert_eq!(
        labels(&trail(&nav(), &LISTS, "/admin", "")),
        [("Dashboard", None)]
    );
    assert_eq!(
        labels(&trail(&nav(), &LISTS, "/admin/users", "")),
        [("Users", None)],
        "an entry's query is not part of its path"
    );
}

/// Of two entries on one path, the one whose query the request carries names the trail.
#[test]
fn the_entry_the_request_holds_names_the_trail() {
    let items = vec![
        NavigationItem::at("Drafts", "/admin/posts?f.status=draft"),
        NavigationItem::at("Blog Posts", "/admin/posts"),
    ];
    assert_eq!(
        labels(&trail(&items, &LISTS, "/admin/posts", "")),
        [("Blog Posts", None)]
    );
    assert_eq!(
        labels(&trail(&items, &LISTS, "/admin/posts", "f.status=draft")),
        [("Drafts", None)]
    );
}

/// A resource's create, view and edit pages link back to its list.
#[test]
fn resource_pages_link_back_to_the_list() {
    assert_eq!(
        labels(&trail(&nav(), &LISTS, "/admin/posts/-/create", "")),
        [("Blog Posts", Some("/admin/posts")), ("Create", None)]
    );
    assert_eq!(
        labels(&trail(&nav(), &LISTS, "/admin/posts/7", "")),
        [("Blog Posts", Some("/admin/posts")), ("View", None)]
    );
    assert_eq!(
        labels(&trail(&nav(), &LISTS, "/admin/posts/7/edit", "")),
        [("Blog Posts", Some("/admin/posts")), ("Edit", None)]
    );
}

/// A page under a record links the record, then names itself.
#[test]
fn a_page_under_a_record_links_the_record() {
    assert_eq!(
        labels(&trail(&nav(), &LISTS, "/admin/posts/7/audit-log", "")),
        [
            ("Blog Posts", Some("/admin/posts")),
            ("View", Some("/admin/posts/7")),
            ("Audit log", None),
        ]
    );
}

/// A path no entry holds has no trail, rather than one that points somewhere else.
#[test]
fn an_unknown_path_has_no_trail() {
    assert!(trail(&nav(), &LISTS, "/elsewhere", "").is_empty());
    assert!(
        trail(&nav(), &LISTS, "/admin-old/posts", "").is_empty(),
        "`/admin` holds only `/admin` and the paths under it"
    );
}

/// A failed page links the entry it is under, then names the failure.
#[test]
fn a_failed_page_names_the_failure_under_its_entry() {
    assert_eq!(
        labels(&failed(&nav(), "/admin/posts/7", "", "Page not found")),
        [
            ("Blog Posts", Some("/admin/posts")),
            ("Page not found", None)
        ]
    );
    assert_eq!(
        labels(&failed(&nav(), "/admin/posts", "", "Access denied")),
        [
            ("Blog Posts", Some("/admin/posts")),
            ("Access denied", None)
        ],
        "the entry links even from its own path"
    );
    assert_eq!(
        labels(&failed(&nav(), "/elsewhere", "", "Page not found")),
        [("Page not found", None)]
    );
}

/// A path under an entry that is no resource's list names its last segment: no record page
/// reads into it.
#[test]
fn a_page_under_another_entry_names_itself() {
    assert_eq!(
        labels(&trail(&nav(), &LISTS, "/admin/reports", "")),
        [("Dashboard", Some("/admin")), ("Reports", None)]
    );
    assert_eq!(
        labels(&trail(&nav(), &LISTS, "/admin/comments/5/edit", "")),
        [("Dashboard", Some("/admin")), ("Edit", None)]
    );
}
