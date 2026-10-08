//! A resource's list: search, sort, cursor pagination, grouping and the empty and error states.

use showcase::models::{PostStatus, Role, User};

use crate::common::{
    body_string, demo_client, empty_users_db, find_href_with, find_pager_href, full_db,
    routers::router_for_tests as router, row_keys, row_titles, seeded_db, user_count,
};

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
    use showcase::models::{Role, User};

    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let seeded = user_count(&db).await;
    let page_size = tablo::table::DEFAULT_PAGE_SIZE.get();
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
                role: Role::Member,
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
    use showcase::models::{Role, User};

    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let seeded = user_count(&db).await;
    let page_size = tablo::table::DEFAULT_PAGE_SIZE.get();
    let extra = page_size - seeded + 1;
    {
        let mut db_q = db.clone();
        for i in 0..extra {
            toasty::create!(User {
                name: format!("User {:02}", i),
                email: format!("desc{:02}@example.com", i),
                role: Role::Member,
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
    use showcase::models::{Role, User};

    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let seeded = user_count(&db).await;
    let page_size = tablo::table::DEFAULT_PAGE_SIZE.get();
    let tied = 30usize;
    {
        let mut db_q = db.clone();
        for i in 0..tied {
            toasty::create!(User {
                name: "Tied".to_string(),
                email: format!("tied{:02}@example.com", i),
                role: Role::Member,
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
        role: showcase::models::Role::Admin,
        active: true,
        age: 30,
        created_at: "2024-02-01T09:30:00Z"
            .parse::<jiff::Timestamp>()
            .expect("timestamp"),
    })
    .exec(&mut tablo::db::db(
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
        html.contains("Clear search</a>"),
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
async fn users_list_renders_its_search_as_a_get_form() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/users").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    // The toolbar is a GET form spelling the list's parameters: the page writes it to the table's
    // query in place, and without JavaScript the browser submits it.
    assert!(
        html.contains("id=\"table-toolbar\" method=\"get\" action=\"/admin/users\"")
            && html.contains("name=\"q\""),
        "users list must render its search as a GET form, got {html}"
    );
}

#[tokio::test]
async fn empty_users_list_shows_no_records_yet() {
    let db = empty_users_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/users").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        html.contains("No records yet"),
        "genuinely empty list must say so: {html}"
    );
    assert!(
        !html.contains("No matches"),
        "empty list must not blame search: {html}"
    );
}

#[tokio::test]
async fn tampered_cursor_shows_in_region_error_with_retry() {
    // A forged cursor fails the load inside the streamed region: the shell
    // (sidebar, heading) survives and the region offers a retry.
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/users?after=forged-cursor").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        html.contains("role=\"alert\""),
        "failed load must announce in place: {html}"
    );
    assert!(
        html.contains("href=\"/admin/users\""),
        "retry must target the bare list: {html}"
    );
    assert!(
        find_href_with(&html, "after=").is_none(),
        "a malformed cursor must not travel into any link: {html}"
    );
    assert!(
        html.contains("data-sidebar"),
        "shell must survive the failed load: {html}"
    );
    assert!(
        !html.contains("No records yet"),
        "a failed load is not an empty result: {html}"
    );
}

#[tokio::test]
async fn stale_cursor_after_concurrent_delete_offers_first_page() {
    // Void window as a real workflow: page 2 exists, its rows are removed
    // elsewhere, and revisiting the stale cursor recovers via first page.
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    {
        let mut db_q = db.clone();
        for i in 0..23 {
            toasty::create!(User {
                name: format!("User {:02}", i),
                email: format!("void{:02}@example.com", i),
                role: Role::Member,
                active: true,
                age: 30,
                created_at: "2024-03-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
            })
            .exec(&mut db_q)
            .await
            .unwrap();
        }
    }
    let page1 = body_string(client.get("/admin/users").await).await;
    let next = find_href_with(&page1, "after=").expect("needs a Next link");
    {
        let mut db_q = db.clone();
        User::filter(User::fields().name().starts_with("User ".to_string()))
            .delete()
            .exec(&mut db_q)
            .await
            .unwrap();
    }
    let resp = client.get(&next).await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        html.contains("Back to first page"),
        "void window must recover: {html}"
    );
}

#[tokio::test]
async fn no_js_fallbacks_cover_search_filter_sort_pager() {
    // Without JavaScript the toolbar is a GET form and the sort and pager
    // links navigate.
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let users = body_string(client.get("/admin/users").await).await;
    assert!(
        users.contains("method=\"get\" action=\"/admin/users\"") && users.contains("name=\"q\""),
        "search needs a GET form: {users}"
    );
    assert!(
        find_href_with(&users, "sort=name").is_some(),
        "sort needs a plain navigation link: {users}"
    );

    let posts = body_string(client.get("/admin/posts").await).await;
    let toolbar = &posts[posts
        .find("id=\"table-toolbar\"")
        .expect("the toolbar form")..];
    assert!(
        toolbar.contains("name=\"f.status\""),
        "the filters submit with the search's GET form: {posts}"
    );

    // Pager preserves state over plain navigation (25 overflow rows force
    // two filtered pages).
    let mut db_q = db.clone();
    let authors = showcase::models::Author::all()
        .exec(&mut db_q)
        .await
        .unwrap();
    for i in 0..25 {
        toasty::create!(showcase::models::Post {
            tenant_id: authors[0].tenant_id,
            title: format!("Nojs Published {:02}", i),
            body: "extra",
            status: PostStatus::Published,
            featured: false,
            created_at: "2024-02-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
            cover_id: None,
            tags: "extra".to_string(),
            seo: showcase::models::Seo {
                title: "Extra".to_string(),
                description: String::new(),
            },
            publication: showcase::models::Publication::Published {
                published_at: Some("2024-02-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap()),
                canonical_url: String::new(),
            },
            author_id: authors[0].id,
        })
        .exec(&mut db_q)
        .await
        .unwrap();
    }
    let page1 = body_string(client.get("/admin/posts?f.status=published").await).await;
    let next = find_href_with(&page1, "after=").expect("filtered Next link");
    assert!(
        next.contains("f.status=published"),
        "pager must preserve filters without JS: {next}"
    );
    assert!(
        !next.contains("&amp;"),
        "the Next link must be followed decoded, got {next}"
    );
}

#[tokio::test]
async fn posts_group_by_status_shows_counts() {
    let db = full_db().await;
    let mut db_q = db.clone();
    // Derived, not literal: the page-local count is the number of
    // published rows in the fixture, so one more seeded post cannot break it.
    let published = showcase::models::Post::filter(
        showcase::models::Post::fields()
            .status()
            .eq(showcase::models::PostStatus::Published),
    )
    .exec(&mut db_q)
    .await
    .unwrap()
    .len();
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/posts?group_by=status").await;
    assert!(
        resp.status().is_success(),
        "group_by should be 200, got {}",
        resp.status()
    );
    let html = body_string(resp).await;
    // The header reads the option's label, with its page-local count. The
    // bare label is not asserted separately: the status SelectFilter renders
    // "Published" and "Draft" as options on every list page, so a label-only
    // check passes with grouping off. `on this page` is emitted only by a
    // group header (`render.rs`), and core pins the ordering and exact
    // "draft (2 on this page)" labels in
    // `group_by_orders_each_row_under_its_own_header`.
    let label = showcase::models::PostStatus::Published.label();
    assert!(
        html.contains(&format!("{label} ({published} on this page)")),
        "missing the published group header in {html}"
    );
}
