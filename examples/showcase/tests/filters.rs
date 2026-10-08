use showcase::models::{Author, Post, PostStatus};

use crate::common::{
    body_string, demo_client, filter_options, find_href_with, full_db, input_value,
    routers::router_for_tests as router, row_titles,
};

#[tokio::test]
async fn posts_filter_widgets_render_typed_controls() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/posts?f.status=published").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    for name in ["status", "featured", "promoted"] {
        assert!(
            filter_options(&html, name).is_some(),
            "missing control for {name} in {html}",
            name = name,
            html = html
        );
    }
    let status = filter_options(&html, "status").expect("status renders");
    assert!(
        status.iter().any(|option| option.value == "draft")
            && status.iter().any(|option| option.value == "published"),
        "status must offer its options in {html}",
        html = html
    );
    assert_eq!(
        status
            .iter()
            .find(|option| option.value == "published")
            .map(|option| option.selected),
        Some(true),
        "the request value stays selected in {html}",
        html = html
    );
    assert!(
        filter_options(&html, "created_at").is_none(),
        "a date filter is not a select in {html}",
        html = html
    );
    assert!(
        input_value(&html, "f.created_at").is_some(),
        "missing date control in {html}",
        html = html
    );
    let toolbar = &html[html.find("id=\"table-toolbar\"").expect("the toolbar form")..];
    assert!(
        toolbar.contains("name=\"f.status\""),
        "the f.<name> controls belong to the toolbar form in {html}"
    );
}

#[tokio::test]
async fn posts_filter_select_status_published() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/posts?f.status=published").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert_eq!(
        row_titles(&html),
        vec!["Hello Toasty".to_string()],
        "the published filter must drop every draft: {html}"
    );
}

/// The featured filter keeps exactly the posts whose flag matches, either way.
#[tokio::test]
async fn the_featured_filter_keeps_the_posts_whose_flag_matches() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let featured = body_string(client.get("/admin/posts?f.featured=true").await).await;
    assert_eq!(row_titles(&featured), ["Hello Toasty"], "{featured}");
    let plain = row_titles(&body_string(client.get("/admin/posts?f.featured=false").await).await);
    let mut db_q = db.clone();
    let featured: Vec<String> = Post::filter(Post::fields().featured().eq(true))
        .exec(&mut db_q)
        .await
        .unwrap()
        .into_iter()
        .map(|post| post.title)
        .collect();
    assert!(
        !plain.is_empty() && plain.iter().all(|title| !featured.contains(title)),
        "false keeps only the posts that are not featured: {plain:?}"
    );
}

#[tokio::test]
async fn posts_filter_date_created_at() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client
        .get("/admin/posts?f.created_at=2024-01-15T09:30:00Z")
        .await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert_eq!(
        row_titles(&html),
        vec!["Hello Toasty".to_string()],
        "the date filter must keep only the matching post: {html}"
    );
}

#[tokio::test]
async fn posts_filter_composes_and() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client
        .get("/admin/posts?f.status=published&f.featured=true")
        .await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert_eq!(
        row_titles(&html),
        vec!["Hello Toasty".to_string()],
        "the AND filter must keep only the both-match row: {html}"
    );
    let resp = client
        .get("/admin/posts?f.status=draft&f.featured=true")
        .await;
    let html = body_string(resp).await;
    assert!(
        row_titles(&html).is_empty(),
        "the AND filter must match nothing: {html}"
    );
}

#[tokio::test]
async fn typo_filter_warns_on_list_but_refuses_export() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let resp = client.get("/admin/posts?f.stauts=published").await;
    assert!(resp.status().is_success(), "typo filter keeps 200");
    let html = body_string(resp).await;
    assert!(
        html.contains("role=\"alert\"") && html.contains("stauts:published"),
        "typo filter must warn, got {html}"
    );

    let resp = client.get("/admin/posts?f.foobar=on").await;
    assert!(resp.status().is_success(), "malformed filter keeps 200");
    let html = body_string(resp).await;
    assert!(
        html.contains("role=\"alert\"") && html.contains("foobar"),
        "malformed filter must banner, got {html}"
    );
    let resp = client.get("/admin/posts/export?f.foobar=on").await;
    assert_eq!(
        resp.status(),
        400,
        "malformed export must refuse, got {}",
        resp.status()
    );

    let resp = client.get("/admin/posts/export?f.stauts=published").await;
    assert_eq!(
        resp.status(),
        400,
        "typo'd export must refuse, got {}",
        resp.status()
    );

    let resp = client.get("/admin/posts/export?f.status=Published").await;
    assert_eq!(
        resp.status(),
        400,
        "rejected-value export must refuse, got {}",
        resp.status()
    );

    let resp = client.get("/admin/posts/export?f.status=published").await;
    assert!(resp.status().is_success(), "valid export must stay 200");
}

#[tokio::test]
async fn posts_filter_with_cursor_paginates_filtered_rows() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let authors = Author::all().exec(&mut db_q).await.unwrap();
    let author_id = authors[0].id;
    let tenant = authors[0].tenant_id;
    for i in 0..25 {
        let title = format!("Published {:02}", i);
        toasty::create!(Post {
            tenant_id: tenant,
            title: title,
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
            author_id: author_id,
        })
        .exec(&mut db_q)
        .await
        .unwrap();
    }
    let resp = client.get("/admin/posts?f.status=published").await;
    assert!(resp.status().is_success());
    let page1 = body_string(resp).await;
    assert!(
        !page1.contains("Second Post"),
        "filtered page 1 must not show drafts: {page1}"
    );
    let next = find_href_with(&page1, "after=").expect("filtered page 1 needs a Next link");
    assert!(
        next.contains("f.status=published"),
        "the pager must preserve filters, got {next}"
    );
    assert!(
        !next.contains("&amp;"),
        "the Next link must be followed decoded, got {next}"
    );
    let resp = client.get(&next).await;
    assert!(resp.status().is_success());
    let page2 = body_string(resp).await;
    assert!(
        !page2.contains("Second Post"),
        "filtered page 2 must not show drafts: {page2}"
    );
    let prev = find_href_with(&page2, "before=").expect("filtered page 2 needs a Previous link");
    assert!(
        prev.contains("f.status=published"),
        "the Previous link must preserve filters, got {prev}"
    );
    assert!(
        !prev.contains("&amp;"),
        "the Previous link must be followed decoded, got {prev}"
    );
    let resp = client.get(&prev).await;
    assert!(resp.status().is_success());
    let back = body_string(resp).await;
    assert!(
        !back.contains("Second Post"),
        "walking back must stay filtered: {back}"
    );
}

async fn create_fixture_post(
    db: &mut toasty::Db,
    author: &Author,
    title: &str,
    status: PostStatus,
    featured: bool,
) {
    toasty::create!(Post {
        tenant_id: author.tenant_id,
        title: title.to_string(),
        body: "Fixture for the promoted facet.".to_string(),
        status,
        featured,
        created_at: "2024-02-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
        cover_id: None,
        tags: "fixture".to_string(),
        seo: showcase::models::Seo {
            title: title.to_string(),
            description: String::new(),
        },
        publication: showcase::models::Publication::Scheduled {
            scheduled_at: Some("2024-02-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap()),
            scheduled_for: None,
        },
        author_id: author.id,
    })
    .exec(&mut *db)
    .await
    .unwrap();
}

#[tokio::test]
async fn posts_filter_variant_promoted_pairs_the_flag_with_status() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let authors = Author::all().exec(&mut db_q).await.unwrap();
    create_fixture_post(
        &mut db_q,
        &authors[0],
        "Featured Draft",
        PostStatus::Draft,
        true,
    )
    .await;
    create_fixture_post(
        &mut db_q,
        &authors[0],
        "Evergreen Roundup",
        PostStatus::Published,
        false,
    )
    .await;

    let cases: [(&str, &[&str]); 6] = [
        ("f.featured=false&q=Evergreen", &["Evergreen Roundup"]),
        ("f.promoted=Backlog&q=Evergreen", &[]),
        ("f.status=draft&q=Featured", &["Featured Draft"]),
        ("f.promoted=Backlog&q=Featured", &[]),
        ("f.promoted=Promoted", &["Hello Toasty"]),
        ("f.promoted=Backlog&q=Second+Post", &["Second Post"]),
    ];
    for (query, expected) in cases {
        let resp = client.get(&format!("/admin/posts?{query}")).await;
        assert!(resp.status().is_success(), "{query} must answer 200");
        let html = body_string(resp).await;
        let expected: Vec<String> = expected.iter().map(|title| title.to_string()).collect();
        assert_eq!(row_titles(&html), expected, "{query}: {html}");
    }
}
