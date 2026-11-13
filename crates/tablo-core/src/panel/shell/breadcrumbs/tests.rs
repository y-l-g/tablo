use topcoat::context::Cx;

use super::*;

fn nav() -> Vec<NavigationItem> {
    vec![
        NavigationItem::at("Dashboard", "/admin"),
        NavigationItem::at("Blog Posts", "/admin/posts"),
        NavigationItem::at("Users", "/admin/users?sort=name"),
    ]
}

/// Loads no record: the unit tests hand the trail what it would have loaded.
fn no_record<'a>(
    _cx: &'a Cx,
    _key: &'a str,
) -> crate::BoxFuture<'a, Option<crate::panel::state::RecordFacts>> {
    Box::pin(async { None })
}

/// A resource's trail at `list`, listed in the sidebar, with no page or relation.
fn resource(list: &str, viewed: bool) -> ResourceTrail {
    ResourceTrail {
        resource: std::any::TypeId::of::<()>(),
        list: list.to_string(),
        label: Text::from("Records"),
        listed: true,
        viewed,
        edited: true,
        pages: Vec::new(),
        owns: Vec::new(),
        record: no_record,
    }
}

/// The resources among [`nav`]'s entries: posts have a detail page and a record page, users
/// neither.
fn resources() -> [ResourceTrail; 2] {
    let mut posts = resource("/admin/posts", true);
    posts.pages = vec![("preview".to_string(), Text::from("Search preview"))];
    [posts, resource("/admin/users", false)]
}

/// [`trail`] under [`resources`], having loaded no record.
fn trail(nav_items: &[NavigationItem], path: &str, query: &str) -> Vec<Crumb> {
    let resources = resources();
    let located = locate(&resources, path);
    super::trail(nav_items, located.as_ref(), &Seen::default(), path, query)
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

/// A record the request may view reads its title, which links its detail page from its edit page.
#[test]
fn a_record_reads_its_title() {
    let resources = resources();
    let seen = Seen {
        title: Some("Hello world".to_string()),
        owner: None,
    };
    let at = |path: &str| super::trail(&nav(), locate(&resources, path).as_ref(), &seen, path, "");
    assert_eq!(
        labels(&at("/admin/posts/7")),
        [("Blog Posts", Some("/admin/posts")), ("Hello world", None)]
    );
    assert_eq!(
        labels(&at("/admin/posts/7/edit")),
        [
            ("Blog Posts", Some("/admin/posts")),
            ("Hello world", Some("/admin/posts/7")),
            ("Edit", None),
        ]
    );
    assert_eq!(
        labels(&at("/admin/users/7/edit")),
        [
            ("Users", Some("/admin/users")),
            ("Hello world", Some("/admin/users/7/edit")),
            ("Edit", None),
        ],
        "a resource with no detail page links the title to its edit page"
    );
}

/// A resource the sidebar does not list sits under its owner: the owner's entry, the owner
/// record and the relation's tab lead the record's own steps.
#[test]
fn an_unlisted_resource_sits_under_its_owner() {
    let mut posts = resource("/admin/posts", true);
    posts.pages = vec![("comments".to_string(), Text::from("Comments"))];
    let mut comments = resource("/admin/comments", false);
    comments.listed = false;
    let resources = [posts, comments];
    let owner = |record| SeenOwner {
        trail: &resources[0],
        record,
        tab: Some("comments".to_string()),
    };
    let path = "/admin/comments/5/edit";
    let located = locate(&resources, path);
    let seen = Seen {
        title: Some("First!".to_string()),
        owner: Some(owner(Some((
            "7".to_string(),
            Some("Hello world".to_string()),
        )))),
    };
    assert_eq!(
        seen.nav_path(path),
        "/admin/posts/7",
        "the sidebar marks the owner"
    );
    assert_eq!(
        labels(&super::trail(&nav(), located.as_ref(), &seen, path, "")),
        [
            ("Blog Posts", Some("/admin/posts")),
            ("Hello world", Some("/admin/posts/7")),
            ("Comments", Some("/admin/posts/7/comments")),
            ("First!", Some("/admin/comments/5/edit")),
            ("Edit", None),
        ]
    );

    let path = "/admin/comments/-/create";
    let located = locate(&resources, path);
    let seen = Seen {
        title: None,
        owner: Some(owner(None)),
    };
    assert_eq!(seen.nav_path(path), "/admin/posts");
    assert_eq!(
        labels(&super::trail(&nav(), located.as_ref(), &seen, path, "")),
        [("Blog Posts", Some("/admin/posts")), ("Create", None)],
        "with no owner record, the owner's entry leads"
    );
}
