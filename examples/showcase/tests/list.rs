//! A resource's list: search, sort, cursor pagination, grouping and the empty and error states.

use showcase::models::PostStatus;
use tablo::testing::{EmptyReason, EmptyTable, empty_table, groups};

use crate::common::{
    body_string, demo_client, empty_users_db, find_href_with, full_db, pager_link,
    routers::router_for_tests as router, row_keys, row_titles, seeded_db, status_label, user_count,
};

#[tokio::test]
async fn renders_search_box_and_sort_links() {
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
    // Without `?sort=` the rows sort by the first sortable column, ascending, and its header
    // says so and links the descending sort; the other sortable columns link their ascending one.
    assert!(
        html.contains("aria-sort=\"ascending\""),
        "missing aria-sort=ascending on the implicit sort in {html}"
    );
    assert!(
        find_href_with(&html, "sort=name&dir=desc").is_some(),
        "missing toggle of the implicit sort in {html}"
    );
    assert!(
        html.contains("aria-sort=\"none\""),
        "missing aria-sort on another sortable column in {html}"
    );
    assert!(
        find_href_with(&html, "sort=email&dir=asc").is_some(),
        "missing sort link in {html}"
    );

    let response = client.get("/admin/users?sort=name&dir=asc").await;
    assert!(
        response.status().is_success(),
        "sorted status {}",
        response.status()
    );
    let sorted = body_string(response).await;
    assert!(
        find_href_with(&sorted, "sort=name&dir=desc").is_some(),
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
        find_href_with(&desc, "sort=name&dir=asc").is_some(),
        "missing toggle back to ascending in {desc}"
    );
}

#[tokio::test]
async fn pagination_walks_page_links() {
    use showcase::models::{Role, User};

    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let seeded = user_count(&db).await;
    let page_size = tablo::DEFAULT_PAGE_SIZE.get();
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
    let next_href =
        pager_link(&page1, "next").unwrap_or_else(|| panic!("page1 missing Next link: {page1}"));
    assert!(
        next_href.contains("page=2"),
        "Next goes to page 2: {next_href}"
    );
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
    let prev_href = pager_link(&page2, "prev")
        .unwrap_or_else(|| panic!("page2 missing Previous link: {page2}"));
    assert!(
        !prev_href.contains("page="),
        "the first page spells no number: {prev_href}"
    );
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
async fn pagination_walks_descending_page_links() {
    use showcase::models::{Role, User};

    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let seeded = user_count(&db).await;
    let page_size = tablo::DEFAULT_PAGE_SIZE.get();
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
        match pager_link(&html, "next") {
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
async fn pagination_keeps_tied_sort_values() {
    use showcase::models::{Role, User};

    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let seeded = user_count(&db).await;
    let page_size = tablo::DEFAULT_PAGE_SIZE.get();
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
    let next = pager_link(&page1, "next").expect("page 2 link");
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

/// Search matches substrings, ignoring case, with wildcards escaped.
#[tokio::test]
async fn search_matches_substrings_and_escapes_wildcards() {
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
    let html = body_string(client.get("/admin/users?q=LOVELACE").await).await;
    assert!(
        html.contains("Ada Lovelace"),
        "search ignores case on every driver: {html}"
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
    .exec(&mut db.clone())
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

/// An empty table says why it is empty: a search that matched nothing offers to clear it, and a
/// table with no records at all blames nothing.
#[tokio::test]
async fn the_empty_state_says_why_the_table_is_empty() {
    let db = seeded_db().await;
    let seeded_router = router(db.clone());
    let client = demo_client(&seeded_router, &db).await;
    let searched = body_string(client.get("/admin/users?q=zzz-none").await).await;
    assert_eq!(
        empty_table(&searched),
        Some(EmptyTable {
            reason: EmptyReason::Search,
            clear: Some("/admin/users".to_string()),
            first_page: None,
        }),
        "{searched}"
    );

    let db = empty_users_db().await;
    let empty_router = router(db.clone());
    let client = demo_client(&empty_router, &db).await;
    let empty = body_string(client.get("/admin/users").await).await;
    assert_eq!(
        empty_table(&empty),
        Some(EmptyTable {
            reason: EmptyReason::NoRecords,
            clear: None,
            first_page: None,
        }),
        "{empty}"
    );
}

/// An empty state's link clears only what emptied the table, keeping the sort, the grouping and
/// whatever else narrowed it; an ignored filter still warns on an empty page.
#[tokio::test]
async fn an_empty_state_clears_only_what_emptied_the_table() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let query = "sort=title&dir=desc&group_by=status";

    let html = body_string(
        client
            .get(&format!("/admin/posts?q=zzz-none&f.featured=true&{query}"))
            .await,
    )
    .await;
    let empty = empty_table(&html).expect("an empty table");
    assert_eq!(empty.reason, EmptyReason::Search, "{html}");
    let clear = empty.clear.expect("a link clearing the search");
    for kept in [
        "sort=title",
        "dir=desc",
        "group_by=status",
        "f.featured=true",
    ] {
        assert!(clear.contains(kept), "{kept} in {clear}");
    }
    assert!(!clear.contains("q="), "{clear}");

    let html = body_string(
        client
            .get(&format!("/admin/posts?f.created_at.to=1900-01-01&{query}"))
            .await,
    )
    .await;
    let empty = empty_table(&html).expect("an empty table");
    assert_eq!(empty.reason, EmptyReason::Filters, "{html}");
    let clear = empty.clear.expect("a link clearing the filters");
    for kept in ["sort=title", "dir=desc", "group_by=status"] {
        assert!(clear.contains(kept), "{kept} in {clear}");
    }
    assert!(!clear.contains("f."), "{clear}");

    let html = body_string(
        client
            .get("/admin/posts?q=zzz-none&f.stauts=published")
            .await,
    )
    .await;
    assert!(empty_table(&html).is_some(), "{html}");
    assert!(
        html.contains("role=\"alert\"") && html.contains("f.stauts=published"),
        "an empty page still warns about the ignored filter: {html}"
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
async fn tampered_cursor_shows_in_region_error_with_retry() {
    // A forged cursor fails the load of a cursor-paged list inside the streamed region: the shell
    // (sidebar, heading) survives and the region offers a retry.
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/comments?after=forged-cursor").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        html.contains("role=\"alert\""),
        "failed load must announce in place: {html}"
    );
    assert!(
        html.contains("href=\"/admin/comments\""),
        "retry must target the bare list: {html}"
    );
    assert!(
        find_href_with(&html, "after=").is_none(),
        "a malformed cursor travels into no link, retry, export or back: {html}"
    );
    assert!(
        html.contains("data-sidebar"),
        "shell must survive the failed load: {html}"
    );
    assert_eq!(
        tablo::testing::empty_table(&html),
        None,
        "a failed load is not an empty result: {html}"
    );
}

#[tokio::test]
async fn stale_cursor_after_concurrent_delete_offers_first_page() {
    // Void window as a real workflow on a cursor-paged list: page 2 exists, its rows are removed
    // elsewhere, and revisiting the stale cursor recovers via first page.
    use showcase::models::{Comment, Post};

    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let post = Post::all().exec(&mut db_q).await.unwrap().remove(0);
    for i in 0..tablo::DEFAULT_PAGE_SIZE.get() + 1 {
        toasty::create!(Comment {
            // Sorts after every fixture comment, so the void rows end the list.
            body: format!("zz void {:02}", i),
            post_id: post.id,
        })
        .exec(&mut db_q)
        .await
        .unwrap();
    }
    let page1 = body_string(client.get("/admin/comments?sort=body").await).await;
    let next = pager_link(&page1, "next").expect("needs a Next link");
    assert!(
        next.contains("after="),
        "a cursor list pages by cursor: {next}"
    );
    Comment::filter(Comment::fields().body().starts_with("zz void ".to_string()))
        .delete()
        .exec(&mut db_q)
        .await
        .unwrap();
    let resp = client.get(&next).await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    let first_page = empty_table(&html)
        .and_then(|empty| empty.first_page)
        .unwrap_or_else(|| panic!("a void window links back to the first page: {html}"));
    assert!(!first_page.contains("after="), "{first_page}");
}

/// A page number past the last page reads as the last page, and one that is no number as the
/// first: a stale or hand-typed link lands on rows, never on an error.
#[tokio::test]
async fn out_of_range_page_numbers_land_on_rows() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let first = row_titles(&body_string(client.get("/admin/users?sort=name").await).await);
    assert!(!first.is_empty());
    for query in ["page=999", "page=0", "page=abc"] {
        let html = body_string(client.get(&format!("/admin/users?sort=name&{query}")).await).await;
        assert_eq!(
            row_titles(&html),
            first,
            "{query} reads as the only page: {html}"
        );
        assert!(
            !html.contains("role=\"alert\""),
            "{query} is no error: {html}"
        );
    }
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
    let next = pager_link(&page1, "next").expect("filtered Next link");
    assert!(
        next.contains("f.status=published"),
        "pager must preserve filters without JS: {next}"
    );
    assert!(
        !next.contains("&amp;"),
        "the Next link must be followed decoded, got {next}"
    );
}

/// Grouped by status, each status's posts run together under one header, the whole list on one
/// page.
#[tokio::test]
async fn posts_group_by_status() {
    let db = full_db().await;
    let mut db_q = db.clone();
    // Derived, not literal, so one more seeded post cannot break it.
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
    let resp = client
        .get("/admin/posts?group_by=status&per_page=100")
        .await;
    assert!(
        resp.status().is_success(),
        "group_by should be 200, got {}",
        resp.status()
    );
    let html = body_string(resp).await;

    // Each status heads one group, every row sits under its own status's header, and the
    // published group holds every published post.
    let posts = showcase::models::Post::all().exec(&mut db_q).await.unwrap();
    let status_of = |title: &str| {
        posts
            .iter()
            .find(|post| post.title == title)
            .unwrap_or_else(|| panic!("no post titled {title}"))
            .status
    };
    let groups = groups(&html);
    assert!(groups.len() > 1, "the fixture spans two statuses: {html}");
    let mut labels: Vec<&str> = groups.iter().map(|group| group.label.as_str()).collect();
    labels.sort_unstable();
    labels.dedup();
    assert_eq!(
        labels.len(),
        groups.len(),
        "each status's rows sit together: {html}"
    );
    for group in &groups {
        for title in group
            .rows
            .iter()
            .filter_map(|row| row.cells.iter().find(|cell| !cell.is_empty()))
        {
            assert_eq!(
                status_label(status_of(title)),
                group.label,
                "{title} sits under its header"
            );
        }
    }
    let label = status_label(PostStatus::Published);
    let published_group = groups
        .iter()
        .find(|group| group.label == label)
        .unwrap_or_else(|| panic!("no {label} group in {html}"));
    assert_eq!(published_group.rows.len(), published, "{html}");
}

/// A grouping the table does not declare groups nothing, and no link carries it on.
#[tokio::test]
async fn an_unknown_grouping_groups_nothing() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get("/admin/posts?group_by=bogus").await).await;
    assert!(!row_titles(&html).is_empty(), "{html}");
    assert!(groups(&html).is_empty(), "{html}");
    assert_eq!(find_href_with(&html, "group_by"), None, "{html}");
}

/// A row opens its record's page from anywhere on the row.
#[tokio::test]
async fn a_row_opens_its_record() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get("/admin/users").await).await;
    let keys = row_keys(&html);
    assert!(!keys.is_empty(), "{html}");
    for key in keys {
        let href = format!("href=\"/admin/users/{key}\"");
        assert!(
            html.split('<')
                .any(|tag| tag.contains("data-record-link") && tag.contains(&href)),
            "the {key} row opens its record: {html}"
        );
    }
}

#[tokio::test]
async fn posts_list_streams_its_rows_behind_the_toolbar() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/posts").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        html.contains("data-topcoat-swap") && html.contains("id=\"table-toolbar\""),
        "posts list must stream its rows into the table, got {html}"
    );
}
