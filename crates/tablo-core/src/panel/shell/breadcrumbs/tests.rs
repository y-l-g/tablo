use super::*;

fn nav() -> Vec<NavigationItem> {
    vec![
        NavigationItem::at("Dashboard", "/admin"),
        NavigationItem::at("Blog Posts", "/admin/posts"),
        NavigationItem::at("Users", "/admin/users?sort=name"),
    ]
}

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
        labels(&trail(&nav(), "/admin/posts")),
        [("Blog Posts", None)]
    );
    assert_eq!(labels(&trail(&nav(), "/admin")), [("Dashboard", None)]);
    assert_eq!(
        labels(&trail(&nav(), "/admin/users")),
        [("Users", None)],
        "an entry's query is not part of its path"
    );
}

/// A resource's create, view and edit pages link back to its list.
#[test]
fn resource_pages_link_back_to_the_list() {
    assert_eq!(
        labels(&trail(&nav(), "/admin/posts/-/create")),
        [("Blog Posts", Some("/admin/posts")), ("Create", None)]
    );
    assert_eq!(
        labels(&trail(&nav(), "/admin/posts/7")),
        [("Blog Posts", Some("/admin/posts")), ("View", None)]
    );
    assert_eq!(
        labels(&trail(&nav(), "/admin/posts/7/edit")),
        [("Blog Posts", Some("/admin/posts")), ("Edit", None)]
    );
}

/// A page under a record links the record, then names itself.
#[test]
fn a_page_under_a_record_links_the_record() {
    assert_eq!(
        labels(&trail(&nav(), "/admin/posts/7/audit-log")),
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
    assert!(trail(&nav(), "/elsewhere").is_empty());
    assert!(
        trail(&nav(), "/admin-old/posts").is_empty(),
        "`/admin` holds only `/admin` and the paths under it"
    );
}
