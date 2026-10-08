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
    assert!(
        find_href_with(&html, "/admin/users/create").is_some(),
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
