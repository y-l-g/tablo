//! The panel's shell around every page: the sidebar, the dashboard and the response headers.

use crate::common::{
    TestClient, body_string, demo_client, find_href_with, routers::router_for_tests as router,
    seeded_db,
};

/// A list page sits in the shell: the sidebar links every resource under its navigation label,
/// the page links its create form, and the rows are the seeded records.
#[tokio::test]
async fn a_list_page_renders_in_the_shell_with_every_resource_in_the_sidebar() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let response = client.get("/admin/users").await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let html = body_string(response).await;

    assert!(
        html.contains("data-sidebar=\"sidebar\""),
        "missing sidebar in {html}"
    );
    for (label, url) in [
        ("Users", "/admin/users"),
        ("Writers", "/admin/authors"),
        ("Blog Posts", "/admin/posts"),
        ("Comments", "/admin/comments"),
    ] {
        assert!(
            html.contains(label) && find_href_with(&html, url).is_some(),
            "the sidebar must link {label} at {url}: {html}"
        );
    }
    let entry = |label: &str| {
        html.find(&format!("title=\"{label}\""))
            .unwrap_or_else(|| panic!("the sidebar lists {label}: {html}"))
    };
    let group = html
        .find(">Blog<")
        .expect("the blog resources render in a Blog group");
    assert!(
        entry("Users") < group,
        "Users renders above the groups: {html}"
    );
    for label in ["Writers", "Blog Posts", "Categories", "Comments"] {
        assert!(
            entry(label) > group,
            "{label} renders in the Blog group: {html}"
        );
    }
    assert!(
        find_href_with(&html, "/admin/users/-/create").is_some(),
        "missing create entry point in {html}"
    );
    let cells: Vec<String> = tablo::testing::rows(&html)
        .into_iter()
        .flat_map(|row| row.cells)
        .collect();
    for value in [
        "Ada Lovelace",
        "ada@example.com",
        "Alan Turing",
        "Grace Hopper",
    ] {
        assert!(
            cells.iter().any(|cell| cell == value),
            "the seeded users must render {value} in a row: {html}"
        );
    }
}

#[tokio::test]
async fn the_panel_root_serves_the_dashboard_with_the_page_entries() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let response = client.get("/admin").await;

    assert_eq!(response.status(), http::StatusCode::OK);
    let html = body_string(response).await;
    assert!(
        html.contains("Dashboard</h1>"),
        "the home page renders: {html}"
    );
    let links: Vec<&str> = html
        .match_indices("data-sidebar=\"menu-button\"")
        .map(|(at, _)| {
            let start = html[..at].rfind('<').unwrap();
            let end = at + html[at..].find("</a>").unwrap();
            &html[start..end]
        })
        .collect();
    let link = |label: &str| {
        links
            .iter()
            .find(|link| link.contains(&format!("title=\"{label}\"")))
            .unwrap_or_else(|| panic!("the sidebar lists {label}: {links:?}"))
    };
    for (label, href) in [("Dashboard", "/admin"), ("Media library", "/admin/media")] {
        assert!(
            link(label).contains(&format!("href=\"{href}\"")),
            "{label} links to {href}: {links:?}"
        );
    }
    let active: Vec<_> = links
        .iter()
        .filter(|link| link.contains("data-active=\"true\""))
        .collect();
    assert_eq!(active, [link("Dashboard")], "one active sidebar entry");
}

/// Every panel response carries `frame-ancestors`.
#[tokio::test]
async fn error_responses_carry_frame_ancestors() {
    use topcoat::router::Body;

    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let csp = |response: &http::Response<Body>| {
        response
            .headers()
            .get(http::header::CONTENT_SECURITY_POLICY)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };

    let response = client.get("/admin/users").await;
    assert!(
        response.status().is_success(),
        "status {}",
        response.status()
    );
    assert!(
        csp(&response).is_some_and(|policy| policy.contains("frame-ancestors")),
        "a panel page must carry the directive"
    );
    // Drain the streamed page before the next request.
    let _ = body_string(response).await;

    let response = client.get("/admin/unknown").await;
    assert_eq!(response.status(), 404);
    assert!(
        csp(&response).is_some_and(|policy| policy.contains("frame-ancestors")),
        "an unmatched route must carry the directive"
    );

    let request = http::Request::builder()
        .method(http::Method::PATCH)
        .uri("/admin/login")
        .body(Body::empty())
        .unwrap();
    let response = router.handle(request).await;
    assert_eq!(response.status(), 405, "PATCH on the login route is a 405");
    assert!(
        csp(&response).is_some_and(|policy| policy.contains("frame-ancestors")),
        "a wrong-method response must carry the directive"
    );

    let anonymous = TestClient::new(&router);
    let response = anonymous.get("/admin/users").await;
    assert_eq!(
        response.status(),
        http::StatusCode::TEMPORARY_REDIRECT,
        "an unauthenticated page request redirects to login"
    );
    assert!(
        response
            .headers()
            .get(http::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|location| location.starts_with("/admin/login")),
        "the redirect must name the login route"
    );
    assert!(
        csp(&response).is_some_and(|policy| policy.contains("frame-ancestors")),
        "the login redirect must carry the directive"
    );
}

/// The sidebar's footer holds the signed-in user's menu: their name and email, the theme choice
/// with the current one checked, the app's own entries, and Sign out, which posts to logout.
#[tokio::test]
async fn the_user_menu_holds_the_theme_the_app_entries_and_sign_out() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get("/admin/users").await).await;

    let footer = &html[html.find("data-user-menu").expect("the user menu")..];
    assert!(
        footer.contains(showcase::models::DEMO_ADMIN_EMAIL),
        "the menu shows the user's email, got {footer}"
    );
    for theme in ["light", "dark", "system"] {
        assert!(
            footer.contains(&format!("data-theme-set=\"{theme}\"")),
            "the menu offers the {theme} theme, got {footer}"
        );
    }
    assert_eq!(
        footer.matches("aria-checked=\"true\"").count(),
        1,
        "one theme is checked, got {footer}"
    );
    assert!(
        find_href_with(footer, "/blog").is_some(),
        "the app's entry links the blog, got {footer}"
    );
    assert!(
        footer.contains("action=\"/admin/logout\""),
        "Sign out posts to logout, got {footer}"
    );
}

/// The top bar's trail names the resource and links its list from a record's pages.
#[tokio::test]
async fn the_trail_links_the_list_from_a_record_page() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let list = body_string(client.get("/admin/users").await).await;
    let edit = find_href_with(&list, "/edit").expect("a row's edit link");
    let html = body_string(client.get(&edit).await).await;

    let trail = &html[html.find("aria-label=\"breadcrumb\"").expect("a trail")..];
    let trail = &trail[..trail.find("</nav>").expect("the trail's end")];
    assert_eq!(
        find_href_with(trail, "/admin/users").as_deref(),
        Some("/admin/users"),
        "the trail links the list, got {trail}"
    );
    assert!(
        trail.contains("aria-current=\"page\"") && trail.contains("Edit"),
        "the trail ends on the edit page, got {trail}"
    );
}

/// A page that fails to load shows the failure inside the shell, under its own URL and status:
/// the sidebar, the user menu, and a trail back to the entry it failed under.
#[tokio::test]
async fn a_failed_page_shows_the_failure_inside_the_shell() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let ken = showcase::models::User::filter(
        showcase::models::User::fields()
            .name()
            .eq("Ken Thompson".to_string()),
    )
    .first()
    .exec(&mut db_q)
    .await
    .unwrap()
    .expect("Ken seed");

    for (path, status, entry) in [
        ("/admin/unknown".to_string(), 404, "/admin"),
        (
            format!("/admin/posts/{}", uuid::Uuid::new_v4()),
            404,
            "/admin/posts",
        ),
        (format!("/admin/users/{}/edit", ken.id), 403, "/admin/users"),
    ] {
        let response = client.get(&path).await;
        assert_eq!(response.status(), status, "{path}");
        let html = body_string(response).await;
        assert!(
            html.contains(&format!("data-panel-error=\"{status}\"")),
            "{path}: the error page, got {html}"
        );
        assert!(
            html.contains("data-sidebar=\"sidebar\"") && html.contains("data-user-menu"),
            "{path}: the shell around it, got {html}"
        );
        let trail = &html[html.find("aria-label=\"breadcrumb\"").expect("a trail")..];
        assert!(
            find_href_with(trail, entry).is_some(),
            "{path}: the trail links {entry}, got {trail}"
        );
    }
}

/// Only a page load shows the error page: an anonymous visitor signs in first, and a request no
/// browser shows keeps the bare status.
#[tokio::test]
async fn only_a_signed_in_page_load_shows_the_error_page() {
    let db = seeded_db().await;
    let router = router(db.clone());

    let response = TestClient::new(&router).get("/admin/unknown").await;
    assert_eq!(
        response.status(),
        http::StatusCode::TEMPORARY_REDIRECT,
        "an unknown path signs the visitor in before it answers"
    );

    let client = demo_client(&router, &db).await;
    let response = client
        .post_form("/admin/unknown", "confirm=1".to_string())
        .await;
    assert_eq!(response.status(), 404, "a POST to an unknown path is a 404");
    let body = body_string(response).await;
    assert!(
        !body.contains("data-panel-error"),
        "a POST keeps the bare 404, got {body}"
    );
}

/// A panel page asks search engines to neither list nor follow it.
#[tokio::test]
async fn panel_pages_ask_search_engines_to_skip_them() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let response = client.get("/admin/users").await;
    assert_eq!(response.status(), http::StatusCode::OK);
    let html = body_string(response).await;
    assert_eq!(
        tablo::testing::robots(&html).as_deref(),
        Some("noindex, nofollow"),
        "the page's robots tag: {html}"
    );
}
