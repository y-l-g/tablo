//! A list's tabs: the rows each admits, the counts their badges show, and the export and the
//! selection of every matching record following the tab the list shows.

use showcase::{
    app::PostResource,
    models::{Post, PostStatus},
    post_actions::PublishPosts,
};
use tablo::testing::{
    EmptyReason, IgnoredFilter, IgnoredReason, TabLink, empty_table, ignored_filters, tab_links,
};

use crate::common::{
    body_string, demo_client, full_db,
    routers::{app_for_tests, router_for_tests as router},
    row_titles,
};

/// The titles of the posts `query` lists, on one page of 100.
async fn listed(client: &tablo::testing::TestClient<'_>, query: &str) -> Vec<String> {
    let html = body_string(
        client
            .get(&format!("/admin/posts?per_page=100&{query}"))
            .await,
    )
    .await;
    row_titles(&html)
}

/// A list without `?tab=` shows the first tab; each badged tab counts the rows it lists, and
/// `?tab=` lists only the rows its tab admits.
#[tokio::test]
async fn a_tab_lists_and_counts_the_rows_it_admits() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let drafts = listed(&client, "f.status=draft").await;
    let published = listed(&client, "f.status=published").await;
    assert!(
        !drafts.is_empty() && !published.is_empty(),
        "the seed holds both"
    );

    let html = body_string(client.get("/admin/posts").await).await;
    let tabs = tab_links(&html);
    assert_eq!(
        tabs.iter()
            .map(|tab| (tab.key.as_str(), tab.active, tab.count))
            .collect::<Vec<_>>(),
        [
            ("all", true, None),
            ("drafts", false, Some(drafts.len() as u64)),
            ("published", false, Some(published.len() as u64)),
        ],
        "{html}"
    );
    assert_eq!(listed(&client, "tab=drafts").await, drafts);
    assert_eq!(listed(&client, "tab=published").await, published);
}

/// A badge counts within the list's search and filters, and a tab link keeps them but leaves the
/// page.
#[tokio::test]
async fn a_badge_counts_within_the_search_and_filters() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(
        client
            .get("/admin/posts?tab=published&q=Second&f.featured=false&per_page=10&page=2")
            .await,
    )
    .await;
    let tabs = tab_links(&html);
    let count = |key: &str| {
        tabs.iter()
            .find(|tab| tab.key == key)
            .and_then(|tab| tab.count)
    };
    for (key, status) in [("published", "published"), ("drafts", "draft")] {
        let query = format!("q=Second&f.featured=false&f.status={status}");
        let expected = listed(&client, &query).await.len() as u64;
        assert_eq!(count(key), Some(expected), "{key} in {html}");
    }
    let all = listed(&client, "").await.len() as u64;
    assert!(
        count("drafts").is_some_and(|drafts| drafts > 0 && drafts < all),
        "the search and filters narrow a non-empty count: {html}"
    );
    let drafts: &TabLink = tabs.iter().find(|tab| tab.key == "drafts").expect("drafts");
    for kept in ["tab=drafts", "q=Second", "f.featured=false", "per_page=10"] {
        assert!(drafts.href.contains(kept), "{kept} in {}", drafts.href);
    }
    assert!(!drafts.href.contains("page=2&") && !drafts.href.ends_with("&page=2"));
    let all = tabs.iter().find(|tab| tab.key == "all").expect("all");
    assert!(
        !all.href.contains("tab="),
        "the first tab spells no tab: {}",
        all.href
    );
}

/// A `?tab=` naming no tab is reported on the list and refused by the export.
#[tokio::test]
async fn an_unknown_tab_is_reported_and_refused() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get("/admin/posts?tab=archived").await).await;
    assert_eq!(
        ignored_filters(&html),
        [IgnoredFilter {
            filter: "tab=archived".to_string(),
            reason: IgnoredReason::Unknown,
        }]
    );
    assert!(tab_links(&html).iter().all(|tab| !tab.active), "{html}");
    let export = client.get("/admin/posts/-/export?tab=archived").await;
    assert_eq!(export.status(), 400);
}

/// The export writes the rows the tab shows.
#[tokio::test]
async fn the_export_follows_the_tab() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let csv = body_string(client.get("/admin/posts/-/export?tab=published").await).await;
    let mut db_q = db.clone();
    for post in Post::all().exec(&mut db_q).await.expect("query posts") {
        let exported = csv.contains(&post.title);
        match post.status {
            PostStatus::Draft => assert!(!exported, "{} in {csv}", post.title),
            PostStatus::Published => {}
        }
    }
}

/// A selection of every matching record holds only the rows the tab shows: publishing the
/// published tab publishes no draft.
#[tokio::test]
async fn a_selection_of_every_match_follows_the_tab() {
    let db = full_db().await;
    let (router, admin) = app_for_tests(db.clone());
    let client = demo_client(&router, &db).await.panel(&admin);
    let mut db_q = db.clone();
    let drafts = Post::filter(Post::fields().status().eq(PostStatus::Draft))
        .exec(&mut db_q)
        .await
        .expect("query drafts");
    assert!(!drafts.is_empty(), "the seed holds drafts");
    let reply = client
        .resource::<PostResource>()
        .bulk_action_matching::<PublishPosts>("tab=published")
        .submit()
        .await;
    assert_ne!(reply.status(), 500);
    for draft in drafts {
        let after = Post::get_by_id(&mut db_q, &draft.id)
            .await
            .expect("the post still exists");
        assert_eq!(after.status, PostStatus::Draft, "{}", draft.title);
    }
    let refused = client
        .resource::<PostResource>()
        .bulk_action_matching::<PublishPosts>("tab=archived")
        .submit()
        .await;
    assert_eq!(refused.status(), 400);
}

/// A tab and a filter no row satisfies together list no row and count none, rather than fail
/// the table.
#[tokio::test]
async fn a_tab_contradicting_a_filter_lists_nothing() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(
        client
            .get("/admin/posts?tab=drafts&f.status=published")
            .await,
    )
    .await;
    assert_eq!(
        empty_table(&html).map(|empty| empty.reason),
        Some(EmptyReason::Filters),
        "{html}"
    );
    let drafts = tab_links(&html).into_iter().find(|tab| tab.key == "drafts");
    assert_eq!(drafts.and_then(|tab| tab.count), Some(0));
}
