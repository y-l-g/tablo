use super::*;
use crate::test_support::Html as _;

#[tokio::test]
async fn shell_escapes_brand_name_and_logo() {
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::navigation::{NavTarget, NavigationItem};

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
    let nav_items = vec![NavigationItem {
        label: "Users".to_string(),
        target: NavTarget::Url("/admin/users".to_string()),
        order: 0,
        icon: None,
    }];
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
async fn shell_dark_mode_sets_html_class_and_toggle() {
    fn cx_with(dark: Option<bool>) -> Cx {
        use topcoat::context::CxTestBuilder;
        let (parts, ()) = http::Request::builder()
            .uri("/admin/users")
            .body(())
            .unwrap()
            .into_parts();
        match dark {
            Some(v) => CxTestBuilder::new()
                .request_context(parts)
                .request_context(dark_panel(v)),
            None => CxTestBuilder::new().request_context(parts),
        }
        .build()
    }

    async fn document_html(cx: &Cx) -> String {
        let slot = view! { cx => "hello" }.boxed().into();
        Panel::layout_shell(cx, slot)
            .single()
            .await
            .unwrap()
            .render(cx)
    }

    let html = document_html(&cx_with(Some(true))).await;
    assert!(
        html.contains("<html class=\"dark\">"),
        "a dark panel must set the html class, got {html}"
    );
    assert!(
        html.contains("data-theme-toggle"),
        "theme toggle must render, got {html}"
    );
    // the build-time default is only that — the pre-paint script
    // can remove the class again when the visitor has stored `light`.
    assert!(
        html.contains("classList.add") && html.contains("classList.remove"),
        "the document must carry the reconciling theme script, got {html}"
    );

    let html = document_html(&cx_with(Some(false))).await;
    assert!(
        html.contains("<html>"),
        "a light panel must not set the dark class, got {html}"
    );

    let html = document_html(&cx_with(None)).await;
    assert!(
        html.contains("<html>"),
        "no panel must not set the dark class, got {html}"
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

    assert_eq!(html_tag(false, "theme=dark").await, "<html class=\"dark\">");
    assert_eq!(
        html_tag(true, "sidebar_state=collapsed; theme=light").await,
        "<html>"
    );
    assert_eq!(html_tag(true, "theme=sepia").await, "<html class=\"dark\">");
}

#[tokio::test]
async fn sidebar_orders_custom_items_by_sort_key() {
    // `order: -1` interleaves a custom item above the
    // resources; ties keep declaration order.
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::navigation::{NavTarget, NavigationItem};

    let (parts, ()) = http::Request::builder()
        .uri("/admin/users")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    let nav_items = vec![
        NavigationItem {
            label: "Users".to_string(),
            target: NavTarget::Url("/admin/users".to_string()),
            order: 0,
            icon: None,
        },
        NavigationItem {
            label: "Showcase".to_string(),
            target: NavTarget::Url("/admin/showcase".to_string()),
            order: -1,
            icon: None,
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
    // The persisted `sidebar_state` cookie seeds the runtime signal's
    // initial value, so the first paint matches the last choice; from
    // hydration on, the browser owns the state (assets/sidebar.js mirrors
    // it back).
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::navigation::{NavTarget, NavigationItem};

    let (parts, ()) = http::Request::builder()
        .uri("/admin/users")
        .header(http::header::COOKIE, "theme=dark; sidebar_state=collapsed")
        .body(())
        .unwrap()
        .into_parts();
    let cx = CxTestBuilder::new().request_context(parts).build();
    let cx_ref = &cx;
    let nav_items = vec![NavigationItem {
        label: "Users".to_string(),
        target: NavTarget::Url("/admin/users".to_string()),
        order: 0,
        icon: None,
    }];
    let slot = view! { cx_ref => "hello" }.boxed().into();
    let html = Panel::render_shell(&cx, &nav_items, "/admin/users", slot, None)
        .await
        .html(&cx)
        .await;
    assert!(
        html.contains("data-state=\"collapsed\"")
            && html.contains("data-collapsible=\"offcanvas\""),
        "collapsed cookie must seed the collapsed state in {html}"
    );
}

/// A home entry at the bare prefix matches every path under it: only the
/// most specific matching entry is active, and only the first of two entries
/// that share a URL.
#[tokio::test]
async fn sidebar_marks_only_the_longest_matching_entry_active() {
    use topcoat::{context::CxTestBuilder, view::view};

    use crate::navigation::NavigationItem;

    let nav_items = vec![
        NavigationItem::at("Dashboard", "/admin"),
        NavigationItem::at("Users", "/admin/users"),
        NavigationItem::at("Overview", "/admin"),
    ];
    for (path, active_label) in [
        ("/admin", "Dashboard"),
        ("/admin/users/create", "Users"),
        ("/admin/media", "Dashboard"),
    ] {
        let (parts, ()) = http::Request::builder()
            .uri(path)
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

/// A panel at `/admin` whose shell starts dark when `dark` is set.
fn dark_panel(dark: bool) -> crate::panel::state::CurrentPanel {
    let mut panel = crate::panel::test_support::panel_state("/admin", crate::Auth::disabled());
    panel.dark_mode = dark;
    crate::panel::test_support::current_panel(panel)
}
