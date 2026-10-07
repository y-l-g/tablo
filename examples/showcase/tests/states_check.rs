use showcase::models::{PostStatus, Role, User};

use crate::common::{
    body_string, demo_client, empty_users_db, find_href_with, full_db,
    routers::router_for_tests as router, seeded_db,
};

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
