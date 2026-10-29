use super::*;

fn nav() -> Vec<NavigationItem> {
    vec![
        NavigationItem::at("Dashboard", "/admin"),
        NavigationItem::at("Blog Posts", "/admin/posts"),
        NavigationItem::at("Users", "/admin/users?sort=name"),
    ]
}

/// The resources among [`nav`]'s entries: posts have a detail page and a record page, users
/// neither.
fn resources() -> [ResourceTrail; 2] {
    [
        ResourceTrail {
            list: "/admin/posts".to_string(),
            viewed: true,
            pages: vec![("preview".to_string(), Text::from("Search preview"))],
        },
        ResourceTrail {
            list: "/admin/users".to_string(),
            viewed: false,
            pages: Vec::new(),
        },
    ]
}

/// [`trail`] under [`resources`].
fn trail(nav_items: &[NavigationItem], path: &str, query: &str) -> Vec<Crumb> {
    let resources = resources();
    super::trail(nav_items, &resources.each_ref(), path, query)
}

/// Each crumb's English label, leaked to compare against literals, and its link.
fn labels(crumbs: &[Crumb]) -> Vec<(&'static str, Option<&str>)> {
    crumbs
        .iter()
        .map(|crumb| {
            let label: &'static str = Box::leak(crumb.label.english().into_boxed_str());
            (label, crumb.href.as_deref())
        })
        .collect()
}

/// The longest entry holding the path names the trail, not the home entry every path sits under.
#[test]
fn the_longest_entry_names_the_trail() {
    assert_eq!(
        labels(&trail(&nav(), "/admin/posts", "")),
        [("Blog Posts", None)]
    );
    assert_eq!(labels(&trail(&nav(), "/admin", "")), [("Dashboard", None)]);
    assert_eq!(
        labels(&trail(&nav(), "/admin/users", "")),
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
        labels(&trail(&items, "/admin/posts", "")),
        [("Blog Posts", None)]
    );
    assert_eq!(
        labels(&trail(&items, "/admin/posts", "f.status=draft")),
        [("Drafts", None)]
    );
}

/// A resource's create, view and edit pages link back to its list.
#[test]
fn resource_pages_link_back_to_the_list() {
    assert_eq!(
        labels(&trail(&nav(), "/admin/posts/-/create", "")),
        [("Blog Posts", Some("/admin/posts")), ("Create", None)]
    );
    assert_eq!(
        labels(&trail(&nav(), "/admin/posts/7", "")),
        [("Blog Posts", Some("/admin/posts")), ("View", None)]
    );
    assert_eq!(
        labels(&trail(&nav(), "/admin/posts/7/edit", "")),
        [("Blog Posts", Some("/admin/posts")), ("Edit", None)]
    );
}

/// A page under a record links the record, then names itself.
#[test]
fn a_page_under_a_record_links_the_record() {
    assert_eq!(
        labels(&trail(&nav(), "/admin/posts/7/audit-log", "")),
        [
            ("Blog Posts", Some("/admin/posts")),
            ("View", Some("/admin/posts/7")),
            ("Audit log", None),
        ]
    );
}

/// A record page reads its label, and links the record's detail page only when there is one.
#[test]
fn a_record_page_reads_its_label() {
    assert_eq!(
        labels(&trail(&nav(), "/admin/posts/7/preview", "")),
        [
            ("Blog Posts", Some("/admin/posts")),
            ("View", Some("/admin/posts/7")),
            ("Search preview", None),
        ]
    );
    assert_eq!(
        labels(&trail(&nav(), "/admin/users/7/audit-log", "")),
        [("Users", Some("/admin/users")), ("Audit log", None)],
        "a resource with no detail page links none"
    );
}

/// A path no entry holds has no trail, rather than one that points somewhere else.
#[test]
fn an_unknown_path_has_no_trail() {
    assert!(trail(&nav(), "/elsewhere", "").is_empty());
    assert!(
        trail(&nav(), "/admin-old/posts", "").is_empty(),
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
        labels(&trail(&nav(), "/admin/reports", "")),
        [("Dashboard", Some("/admin")), ("Reports", None)]
    );
    assert_eq!(
        labels(&trail(&nav(), "/admin/comments/5/edit", "")),
        [("Dashboard", Some("/admin")), ("Edit", None)]
    );
}

/// A segment the URL percent-encodes reads decoded.
#[test]
fn an_encoded_segment_reads_decoded() {
    assert_eq!(
        labels(&trail(&nav(), "/admin/reports/Q1%20sales", "")),
        [("Dashboard", Some("/admin")), ("Q1 sales", None)]
    );
}
