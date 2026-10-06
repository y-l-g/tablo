use crate::common::{
    TestClient, body_string, demo_client, find_href_with, find_pager_href,
    routers::router_for_tests as router, row_keys, row_titles, seeded_db, user_count,
};

#[tokio::test]
async fn admin_resource_list_page_serve_seeded_users() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let response = client.get("/admin/users").await;

    assert!(
        response.status().is_success(),
        "status {}",
        response.status()
    );
    let html = body_string(response).await;

    assert!(
        html.contains("<html>"),
        "showcase must paint light by default in {html}"
    );
    assert!(
        html.contains("data-sidebar=\"sidebar\"") || html.contains("data-sidebar=\"menu\""),
        "missing sidebar in {html}"
    );
    assert!(html.contains("Users"), "missing Users label in {html}");
    assert!(
        find_href_with(&html, "/admin/users").is_some(),
        "missing navigation url in {html}"
    );
    assert!(html.contains("Writers"), "missing Writers label in {html}");
    assert!(
        find_href_with(&html, "/admin/authors").is_some(),
        "missing Writers navigation url in {html}"
    );
    assert!(
        html.contains("Blog Posts"),
        "missing Blog Posts label in {html}"
    );
    assert!(
        find_href_with(&html, "/admin/posts").is_some(),
        "missing Blog Posts navigation url in {html}"
    );
    assert!(
        html.contains("Comments"),
        "missing Comments label in {html}"
    );
    assert!(
        find_href_with(&html, "/admin/comments").is_some(),
        "missing Comments navigation url in {html}"
    );
    assert!(
        !html.contains("f.status=published"),
        "the redundant Published saved view must be gone: {html}"
    );
    assert!(
        !html.contains("href=\"/admin/showcase\""),
        "showcase navigation must be gone in {html}"
    );
    assert!(html.contains("Users</h1>"), "missing heading in {html}");
    assert!(
        find_href_with(&html, "/admin/users/create").is_some() && !html.contains("Create Users"),
        "missing singular create entry point in {html}"
    );
    let cells: Vec<String> = tablo_test::rows(&html)
        .into_iter()
        .flat_map(|row| row.cells)
        .collect();
    for value in [
        "Ada Lovelace",
        "ada@example.com",
        "Alan Turing",
        "alan@example.com",
        "Grace Hopper",
    ] {
        assert!(
            cells.iter().any(|cell| cell == value),
            "the seeded users must render {value} in a row: {html}"
        );
    }
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

#[tokio::test]
async fn admin_root_serves_the_dashboard_with_the_page_entries() {
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
    for (label, href) in [
        ("Dashboard", "/admin"),
        ("Media library", "/admin/media"),
        ("Live activity", "/admin/live"),
    ] {
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

#[tokio::test]
async fn removed_showcase_routes_are_not_found() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    for path in [
        "/admin/showcase",
        "/admin/showcase/ui",
        "/admin/showcase/dialog",
        "/admin/showcase/panel",
        "/admin/showcase/resource",
        "/admin/showcase/schema",
        "/admin/showcase/table",
        "/admin/showcase/db",
    ] {
        let response = client.get(path).await;
        assert_eq!(response.status(), 404, "{path} should be gone");
    }
}

#[tokio::test]
async fn admin_list_renders_search_box_and_sort_links() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let response = client.get("/admin/users").await;
    assert!(
        response.status().is_success(),
        "status {}",
        response.status()
    );
    let html = body_string(response).await;
    assert!(
        html.contains("<form") && html.contains("name=\"q\""),
        "missing search form in {html}"
    );
    assert!(
        html.contains("type=\"search\""),
        "missing search input type in {html}"
    );
    assert!(
        html.contains("sort=name&amp;dir=asc"),
        "missing sort link in {html}"
    );
    assert!(
        html.contains("aria-sort=\"none\""),
        "missing aria-sort on sortable column in {html}"
    );

    let response = client.get("/admin/users?sort=name&dir=asc").await;
    assert!(
        response.status().is_success(),
        "sorted status {}",
        response.status()
    );
    let sorted = body_string(response).await;
    assert!(
        sorted.contains("sort=name&amp;dir=desc"),
        "missing sort toggle link in {sorted}"
    );
    assert!(
        sorted.contains("aria-sort=\"ascending\""),
        "missing aria-sort=ascending in {sorted}"
    );

    let response = client.get("/admin/users?sort=name&dir=desc").await;
    assert!(
        response.status().is_success(),
        "desc status {}",
        response.status()
    );
    let desc = body_string(response).await;
    assert!(
        desc.contains("aria-sort=\"descending\""),
        "missing aria-sort=descending in {desc}"
    );
    assert!(
        desc.contains("sort=name&amp;dir=asc"),
        "missing toggle back to ascending in {desc}"
    );
}

#[tokio::test]
async fn admin_list_pagination_walks_cursor_links() {
    use showcase::models::User;

    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let seeded = user_count(&db).await;
    let page_size = tablo_core::table::DEFAULT_PAGE_SIZE.get();
    let extra = page_size - seeded + 1;
    let last = format!("User {:02}", extra - 1);
    {
        let mut db_q = db.clone();
        for i in 0..extra {
            let name = format!("User {:02}", i);
            let email = format!("user{:02}@example.com", i);
            toasty::create!(User {
                name: name,
                email: email,
                role: "member",
                active: true,
                age: 30,
                created_at: "2024-03-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
            })
            .exec(&mut db_q)
            .await
            .unwrap();
        }
    }
    let response = client.get("/admin/users?sort=name&dir=asc").await;
    let page1 = body_string(response).await;
    let page1_titles = row_titles(&page1);

    for name in ["Ada Lovelace", "Alan Turing", "Grace Hopper"] {
        assert!(
            page1_titles.iter().any(|title| title == name),
            "page1 missing {name}: {page1}"
        );
    }
    assert!(
        !page1.contains(&last),
        "page1 must not show the last overflow row {last} (page size {page_size}): {page1}"
    );
    let next_href = find_href_with(&page1, "after=")
        .unwrap_or_else(|| panic!("page1 missing Next (after=) link: {page1}"));
    assert!(
        !next_href.contains("&amp;"),
        "the Next link must be followed decoded, got {next_href}"
    );
    assert!(
        next_href.contains("sort=name") && next_href.contains("dir=asc"),
        "the pager must preserve the sort state, got {next_href}"
    );

    let response = client.get(&next_href).await;
    assert!(
        response.status().is_success(),
        "page2 status {}",
        response.status()
    );
    let page2 = body_string(response).await;
    assert!(
        page2.contains(&last),
        "page2 missing the overflow row {last}: {page2}"
    );
    assert!(
        !page2.contains("Ada Lovelace") && !page2.contains("Alan Turing"),
        "page2 must not repeat page 1 rows: {page2}"
    );
    assert!(
        find_href_with(&page2, "before=").is_some(),
        "page2 missing Previous (before=) link: {page2}"
    );

    let prev_href = find_href_with(&page2, "before=").unwrap();
    assert!(
        !prev_href.contains("&amp;"),
        "the Previous link must be followed decoded, got {prev_href}"
    );
    let response = client.get(&prev_href).await;
    assert!(
        response.status().is_success(),
        "page1-again status {}",
        response.status()
    );
    let page1_again = body_string(response).await;
    assert_eq!(
        row_titles(&page1_again),
        page1_titles,
        "following Previous must restore page 1 unchanged"
    );
}

#[tokio::test]
async fn admin_list_pagination_walks_descending_cursor_links() {
    use showcase::models::User;

    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let seeded = user_count(&db).await;
    let page_size = tablo_core::table::DEFAULT_PAGE_SIZE.get();
    let extra = page_size - seeded + 1;
    {
        let mut db_q = db.clone();
        for i in 0..extra {
            toasty::create!(User {
                name: format!("User {:02}", i),
                email: format!("desc{:02}@example.com", i),
                role: "member",
                active: true,
                age: 30,
                created_at: "2024-03-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
            })
            .exec(&mut db_q)
            .await
            .unwrap();
        }
    }
    let total = user_count(&db).await;
    assert!(total > page_size, "the fixture must span two pages");

    let mut url = "/admin/users?sort=name&dir=desc".to_string();
    let mut names = Vec::new();
    let mut keys = Vec::new();
    for _ in 0..8 {
        let response = client.get(&url).await;
        assert!(response.status().is_success(), "GET {url}");
        let html = body_string(response).await;
        assert!(
            html.contains("sort=name") && html.contains("dir=desc"),
            "the pager must keep the descending state, got {html}"
        );
        names.extend(row_titles(&html));
        keys.extend(row_keys(&html));
        match find_pager_href(&html, "after=") {
            Some(next) => {
                assert!(!next.contains("&amp;"), "the link must be decoded: {next}");
                url = next;
            }
            None => break,
        }
    }
    assert_eq!(names.len(), total, "the walk must cover every row");
    let mut descending = names.clone();
    descending.sort();
    descending.reverse();
    assert_eq!(names, descending, "the pages must stay in descending order");
    let unique: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(
        unique.len(),
        total - 1,
        "no row may appear on two pages of a descending walk"
    );
}

#[tokio::test]
async fn admin_list_pagination_keeps_tied_sort_values() {
    use showcase::models::User;

    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let seeded = user_count(&db).await;
    let page_size = tablo_core::table::DEFAULT_PAGE_SIZE.get();
    let tied = 30usize;
    {
        let mut db_q = db.clone();
        for i in 0..tied {
            toasty::create!(User {
                name: "Tied".to_string(),
                email: format!("tied{:02}@example.com", i),
                role: "member",
                active: true,
                age: 30,
                created_at: "2024-03-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
            })
            .exec(&mut db_q)
            .await
            .unwrap();
        }
    }
    let total = user_count(&db).await;

    let page1 = body_string(client.get("/admin/users?sort=name&dir=asc").await).await;
    let page1_titles = row_titles(&page1);
    assert_eq!(page1_titles.len(), page_size, "page 1 must be full");
    assert_eq!(
        page1_titles.iter().filter(|title| *title == "Tied").count(),
        page_size - seeded,
        "the tied group must start inside page 1: {page1}"
    );
    let next = find_pager_href(&page1, "after=").expect("page 2 link");
    let page2 = body_string(client.get(&next).await).await;
    let page2_titles = row_titles(&page2);
    assert_eq!(
        page2_titles.len(),
        total - page_size,
        "page 2 holds the rest"
    );
    assert!(
        page2_titles.iter().all(|title| title == "Tied"),
        "page 2 must continue the tied group: {page2}"
    );

    let mut keys = row_keys(&page1);
    keys.extend(row_keys(&page2));
    let unique: std::collections::HashSet<_> = keys.iter().collect();
    assert_eq!(
        unique.len(),
        total - 1,
        "the tie-breaker must not skip or repeat a row"
    );
}

/// Search matches substrings with wildcards escaped.
#[tokio::test]
async fn admin_list_search_matches_substrings_and_escapes_wildcards() {
    let db = seeded_db().await;
    let router = router(db.clone());

    let client = demo_client(&router, &db).await;
    let response = client.get("/admin/users?q=vela").await;
    assert!(response.status().is_success());
    let html = body_string(response).await;
    assert!(
        html.contains("Ada Lovelace"),
        "a mid-string term must match: {html}"
    );
    assert!(
        !html.contains("Alan Turing"),
        "a mid-string term must not match the other rows: {html}"
    );

    toasty::create!(showcase::models::User {
        name: "100% Ada".to_string(),
        email: "percent@example.com".to_string(),
        role: "admin".to_string(),
        active: true,
        age: 30,
        created_at: "2024-02-01T09:30:00Z"
            .parse::<jiff::Timestamp>()
            .expect("timestamp"),
    })
    .exec(&mut tablo_core::db::db(
        &topcoat::context::CxTestBuilder::new()
            .app_context(db.clone())
            .build(),
    ))
    .await
    .expect("seed the percent user");

    let response = client.get("/admin/users?q=100%25").await;
    let html = body_string(response).await;
    assert!(
        html.contains("100% Ada"),
        "an escaped literal percent must match its row: {html}"
    );
    assert!(
        !html.contains("Ada Lovelace") && !html.contains("Grace Hopper"),
        "an escaped percent must not act as a wildcard: {html}"
    );
}

#[tokio::test]
async fn admin_list_empty_search_shows_no_results_with_clear() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let response = client.get("/admin/users?q=zzz-none").await;
    assert!(
        response.status().is_success(),
        "status {}",
        response.status()
    );
    let html = body_string(response).await;
    assert!(
        html.contains("data-search-clear"),
        "search-empty state must offer the clear link: {html}"
    );
    assert!(
        !html.contains("No records yet"),
        "search-empty state must not claim no records: {html}"
    );
    assert!(
        !html.contains("Create record"),
        "dead Create button must stay gone: {html}"
    );
}
#[tokio::test]
async fn users_list_renders_live_search_host_with_get_fallback() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/users").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        html.contains("data-live-search"),
        "users list must render the live host, got {html}"
    );
    assert!(
        html.contains("<noscript>"),
        "live list must keep the GET fallback, got {html}"
    );
}
