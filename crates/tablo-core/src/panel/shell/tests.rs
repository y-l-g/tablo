use super::*;
use crate::test_support::Html as _;

#[tokio::test]
async fn shell_escapes_brand_name_and_logo() {
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::navigation::NavigationItem;

    // Attribute-injection safety rests on `view!` escaping:
    // lock it with a hostile brand on both render paths (header + sidebar).
    let (parts, ()) = http::Request::builder()
        .uri("/admin/users")
        .body(())
        .unwrap()
        .into_parts();
    let mut panel = crate::panel::test_support::panel_state("/admin", crate::Auth::disabled());
    panel.brand =
        Some(Brand::new("<script>alert(1)</script>").logo("\"><script>alert(2)</script>"));
    let cx = CxTestBuilder::new()
        .request_context(parts)
        .request_context(crate::panel::test_support::current_panel(panel))
        .build();
    let nav_items = vec![NavigationItem::at("Users", "/admin/users")];
    let cx_ref = &cx;
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &nav_items, "/admin/users", slot, None)
        .await
        .html(&cx)
        .await;
    assert!(
        !html.contains("\"><script>"),
        "brand must not break out of attributes, got {html}"
    );
    assert!(
        html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "missing escaped brand text, got {html}"
    );
    assert!(
        html.contains("&quot;"),
        "attribute quotes must be escaped, got {html}"
    );
}

#[tokio::test]
async fn theme_cookie_overrides_the_dark_mode_default() {
    // Runtime navigation copies `<html>`'s attributes from the next page
    // and runs no script, so the server must render the stored choice.
    use topcoat::context::CxTestBuilder;

    async fn html_tag(default_dark: bool, cookie: &str) -> String {
        let (parts, ()) = http::Request::builder()
            .uri("/admin/users")
            .header(http::header::COOKIE, cookie)
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new()
            .request_context(parts)
            .request_context(dark_panel(default_dark))
            .build();
        let cx_ref = &cx;
        let slot = view! { cx_ref => "hello" }.boxed().into();
        let html = Panel::layout_shell(&cx, slot)
            .single()
            .await
            .unwrap()
            .render(&cx);
        let start = html.find("<html").unwrap();
        html[start..=start + html[start..].find('>').unwrap()].to_string()
    }

    assert_eq!(
        html_tag(false, "tablo_theme=dark").await,
        "<html class=\"dark\">"
    );
    assert_eq!(
        html_tag(true, "tablo_sidebar=collapsed; tablo_theme=light").await,
        "<html>"
    );
    assert_eq!(
        html_tag(true, "tablo_theme=sepia").await,
        "<html class=\"dark\">"
    );
    // Another app's cookie on the same host decides nothing, even listed first.
    assert_eq!(
        html_tag(true, "theme=light; xtablo_theme=light").await,
        "<html class=\"dark\">"
    );
    assert_eq!(
        html_tag(false, "theme=light; tablo_theme=dark").await,
        "<html class=\"dark\">"
    );
    // Under `system`, the scheme the theme script mirrors decides, and the default stands in
    // until it has.
    assert_eq!(
        html_tag(false, "tablo_theme=system; tablo_scheme=dark").await,
        "<html class=\"dark\">"
    );
    assert_eq!(
        html_tag(true, "tablo_theme=system; tablo_scheme=light").await,
        "<html>"
    );
    assert_eq!(
        html_tag(true, "tablo_theme=system").await,
        "<html class=\"dark\">"
    );
}

#[tokio::test]
async fn sidebar_orders_custom_items_by_sort_key() {
    // `order: -1` interleaves a custom item above the
    // resources; ties keep declaration order.
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::navigation::NavigationItem;

    let (parts, ()) = http::Request::builder()
        .uri("/admin/users")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    let nav_items = vec![
        NavigationItem::at("Users", "/admin/users"),
        NavigationItem {
            order: -1,
            ..NavigationItem::at("Showcase", "/admin/showcase")
        },
    ];
    let cx_ref = &cx;
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &nav_items, "/admin/users", slot, None)
        .await
        .html(&cx)
        .await;
    let showcase_at = html.find("Showcase").expect("custom item renders");
    let users_at = html.find("Users").expect("resource item renders");
    assert!(
        showcase_at < users_at,
        "an order: -1 custom item must precede resources, got {html}"
    );
}

#[tokio::test]
async fn collapsed_sidebar_cookie_seeds_the_signal() {
    // The persisted `tablo_sidebar` cookie seeds the runtime signal's
    // initial value, so the first paint matches the last choice; from
    // hydration on, the browser owns the state (assets/sidebar.js mirrors
    // it back).
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::navigation::NavigationItem;

    let (parts, ()) = http::Request::builder()
        .uri("/admin/users")
        .header(
            http::header::COOKIE,
            "tablo_theme=dark; tablo_sidebar=collapsed",
        )
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    let cx_ref = &cx;
    let nav_items = vec![NavigationItem::at("Users", "/admin/users")];
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &nav_items, "/admin/users", slot, None)
        .await
        .html(&cx)
        .await;
    assert!(
        html.contains("data-state=\"collapsed\"") && html.contains("data-collapsible=\"icon\""),
        "collapsed cookie must seed the collapsed state in {html}"
    );
}

/// A home entry at the bare prefix matches every path under it, so only the most specific
/// matching entry is active. Of two entries on one path, the one whose query the request
/// carries is active, and the first declared wins a tie.
#[tokio::test]
async fn sidebar_marks_only_the_longest_matching_entry_active() {
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::navigation::NavigationItem;

    // The query entry comes first: declaration order must not decide between it and `Users`.
    let nav_items = vec![
        NavigationItem::at("Drafts", "/admin/users?f.status=draft"),
        NavigationItem::at("Dashboard", "/admin"),
        NavigationItem::at("Users", "/admin/users"),
        NavigationItem::at("Overview", "/admin"),
    ];
    for (uri, active_label) in [
        ("/admin", "Dashboard"),
        ("/admin/users", "Users"),
        ("/admin/users?f.status=review", "Users"),
        ("/admin/users?f.status=draft", "Drafts"),
        ("/admin/users/-/create", "Users"),
        ("/admin/media", "Dashboard"),
    ] {
        let path = uri.split_once('?').map_or(uri, |(path, _)| path);
        let (parts, ()) = http::Request::builder()
            .uri(uri)
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new().request_context(parts).build();
        let cx_ref = &cx;
        let slot = view! { cx_ref => "hello" }.boxed().into();
        let html = Panel::render_shell(&cx, &nav_items, path, slot, None)
            .await
            .html(&cx)
            .await;
        let active: Vec<&str> = html
            .match_indices("data-active=\"true\"")
            .map(|(at, _)| {
                let start = html[..at].rfind('<').unwrap();
                // The whole link: an attribute value can hold a `>`.
                let end = at + html[at..].find("</a>").unwrap();
                &html[start..end]
            })
            .collect();
        assert_eq!(active.len(), 1, "one active entry on {path}: {active:?}");
        assert!(
            active[0].contains(&format!("title=\"{active_label}\"")),
            "{active_label} is active on {path}: {active:?}"
        );
    }
}

/// The ungrouped entries render first under the default label, then one labelled group per
/// `group` in the order of its first entry; the active entry is found across groups.
#[tokio::test]
async fn sidebar_groups_entries_under_their_labels() {
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::navigation::NavigationItem;

    let mut settings = NavigationItem::at("Settings", "/admin/settings").group("System");
    settings.order = -1;
    let nav_items = vec![
        NavigationItem::at("Posts", "/admin/posts").group("Content"),
        NavigationItem::at("Users", "/admin/users"),
        NavigationItem::at("Tags", "/admin/tags").group("Content"),
        settings,
    ];
    let render = |nav_items: Vec<NavigationItem>| async move {
        let (parts, ()) = http::Request::builder()
            .uri("/admin/tags")
            .body(())
            .unwrap()
            .into_parts();
        let cx = CxTestBuilder::new().request_context(parts).build();
        let cx_ref = &cx;
        let slot = view! { cx_ref => "hello" }.boxed().into();
        Panel::render_shell(&cx, &nav_items, "/admin/tags", slot, None)
            .await
            .html(&cx)
            .await
    };

    let html = render(nav_items.clone()).await;
    let at = |needle: &str| {
        html.find(needle)
            .unwrap_or_else(|| panic!("{needle} renders: {html}"))
    };
    let order = [
        at(">Navigation<"),
        at("title=\"Users\""),
        at(">System<"),
        at("title=\"Settings\""),
        at(">Content<"),
        at("title=\"Posts\""),
        at("title=\"Tags\""),
    ];
    assert!(order.is_sorted(), "sections and entries in order: {html}");
    assert_eq!(html.matches(">Content<").count(), 1, "one Content group");
    let active = html.find("data-active=\"true\"").expect("an active entry");
    assert!(
        html[active..].find("title=\"Tags\"") < html[active..].find("</a>"),
        "Tags is active: {html}"
    );

    let grouped = nav_items
        .into_iter()
        .filter(|item| item.group.is_some())
        .collect();
    let html = render(grouped).await;
    assert!(
        !html.contains(">Navigation<"),
        "no empty default group: {html}"
    );
}

/// A panel at `/admin` whose shell starts dark when `dark` is set.
fn dark_panel(dark: bool) -> crate::panel::state::CurrentPanel {
    let mut panel = crate::panel::test_support::panel_state("/admin", crate::Auth::disabled());
    panel.dark_mode = dark;
    crate::panel::test_support::current_panel(panel)
}
