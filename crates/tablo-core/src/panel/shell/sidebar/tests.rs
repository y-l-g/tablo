use super::*;

/// A custom entry ordered `-1` interleaves above the resources, ties keeping declaration order;
/// the ungrouped entries come first under the default label, then one group per label in the
/// order of its first entry, and no default group shows when every entry has one.
#[test]
fn the_sidebar_orders_entries_and_groups_them_under_their_labels() {
    let mut settings = NavigationItem::at("Settings", "/admin/settings").group("System");
    settings.order = -1;
    let mut dashboard = NavigationItem::at("Dashboard", "/admin");
    dashboard.order = -1;
    let nav_items = vec![
        NavigationItem::at("Posts", "/admin/posts").group("Content"),
        NavigationItem::at("Users", "/admin/users"),
        NavigationItem::at("Tags", "/admin/tags").group("Content"),
        settings,
        dashboard,
    ];
    let labels = |nav_items: &[NavigationItem]| -> Vec<(String, Vec<(usize, String)>)> {
        sidebar_sections(in_sidebar_order(nav_items))
            .into_iter()
            .map(|(label, entries)| {
                let entries = entries
                    .into_iter()
                    .map(|(index, item)| (index, item.label))
                    .collect();
                (label, entries)
            })
            .collect()
    };
    let section = |label: &str, entries: &[(usize, &str)]| {
        let entries = entries
            .iter()
            .map(|(index, entry)| (*index, entry.to_string()))
            .collect();
        (label.to_string(), entries)
    };
    assert_eq!(
        labels(&nav_items),
        [
            section("Navigation", &[(1, "Dashboard"), (3, "Users")]),
            section("System", &[(0, "Settings")]),
            section("Content", &[(2, "Posts"), (4, "Tags")]),
        ]
    );
    let grouped: Vec<_> = nav_items
        .into_iter()
        .filter(|item| item.group.is_some())
        .collect();
    assert_eq!(
        labels(&grouped),
        [
            section("System", &[(0, "Settings")]),
            section("Content", &[(1, "Posts"), (2, "Tags")]),
        ]
    );
}
