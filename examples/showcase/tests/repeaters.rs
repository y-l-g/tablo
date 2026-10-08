//! A repeater: the post form's links, a row per item of a `#[document]` list.

use showcase::models::{Author, Link, Post};
use tablo::testing::TestClient;

use crate::common::{body_string, demo_client, full_db, post_count, routers::router_for_tests};

/// The rows' keys a post with `title` posts, links listed in `order`.
async fn create_body(db: &toasty::Db, title: &str, rows: &str) -> String {
    let mut db = db.clone();
    let author = &Author::all().exec(&mut db).await.unwrap()[0];
    format!("title={title}&author_id={}&{rows}", author.id)
}

async fn post_titled(db: &toasty::Db, title: &str) -> Option<Post> {
    let mut db = db.clone();
    Post::filter(Post::fields().title().eq(title.to_string()))
        .first()
        .exec(&mut db)
        .await
        .unwrap()
}

fn link(label: &str, url: &str) -> Link {
    Link {
        label: label.to_string(),
        url: url.to_string(),
    }
}

/// Creates a post titled `title` holding the links "Docs" then "Blog".
async fn create_with_links(client: &TestClient<'_>, db: &toasty::Db, title: &str) -> Post {
    let body = create_body(
        db,
        title,
        "links=0,1&links.0.label=Docs&links.0.url=https%3A%2F%2Fdocs.example\
         &links.1.label=Blog&links.1.url=https%3A%2F%2Fblog.example",
    )
    .await;
    let resp = client.submit("/admin/posts/create", &body).await;
    assert!(resp.status().is_redirection(), "the create redirects");
    post_titled(db, title).await.expect("the post is created")
}

#[tokio::test]
async fn the_rows_save_in_the_order_the_form_lists_them() {
    let db = full_db().await;
    let router = router_for_tests(db.clone());
    let client = demo_client(&router, &db).await;
    // Row 3 was added after row 0 and moved above it; the gap is a removed row.
    let body = create_body(
        &db,
        "Ordered",
        "links=3,0&links.0.label=Docs&links.0.url=https%3A%2F%2Fdocs.example\
         &links.3.label=Blog&links.3.url=https%3A%2F%2Fblog.example",
    )
    .await;
    let resp = client.submit("/admin/posts/create", &body).await;
    assert!(
        resp.status().is_redirection(),
        "the create redirects, got {}: {}",
        resp.status(),
        body_string(resp).await
    );
    let post = post_titled(&db, "Ordered")
        .await
        .expect("the post is created");
    assert_eq!(
        post.links,
        vec![
            link("Blog", "https://blog.example"),
            link("Docs", "https://docs.example")
        ]
    );
}

#[tokio::test]
async fn the_edit_page_renders_a_row_per_link_and_a_blank_row_to_copy() {
    let db = full_db().await;
    let router = router_for_tests(db.clone());
    let client = demo_client(&router, &db).await;
    let post = create_with_links(&client, &db, "Hydrated").await;

    let html = body_string(client.get(&format!("/admin/posts/{}/edit", post.id)).await).await;
    for needle in [
        r#"data-repeater="links""#,
        r#"data-repeater-next="2""#,
        r#"name="links" value="0,1""#,
        r#"name="links.0.label""#,
        r#"value="Docs""#,
        r#"name="links.1.url""#,
        r#"value="https://blog.example""#,
        // The blank row the add button copies, numbered by the browser.
        r#"name="links.__row__.label""#,
        r#"data-repeater-add"#,
    ] {
        assert!(html.contains(needle), "no {needle} in the edit page");
    }
    let rows = html.matches("data-repeater-row=").count();
    assert_eq!(rows, 3, "two rows and the blank one");
}

#[tokio::test]
async fn a_refused_row_renders_its_error_on_its_own_control_and_writes_nothing() {
    let db = full_db().await;
    let router = router_for_tests(db.clone());
    let client = demo_client(&router, &db).await;
    let before = post_count(&db).await;
    let body = create_body(
        &db,
        "Refused",
        "links=4,2&links.4.label=Docs&links.4.url=https%3A%2F%2Fdocs.example\
         &links.2.label=Blog&links.2.url=",
    )
    .await;
    let resp = client.submit("/admin/posts/create", &body).await;
    assert!(
        resp.status().is_success(),
        "the refusal re-renders the form"
    );
    let html = body_string(resp).await;
    // The rows render again in the posted order, numbered from 0.
    assert_eq!(
        tablo::testing::field_error(&html, "links.1.url").as_deref(),
        Some("URL is required"),
        "the second row's URL names its refusal"
    );
    assert_eq!(tablo::testing::field_error(&html, "links.0.url"), None);
    assert!(html.contains(r#"name="links" value="0,1""#));
    assert!(html.contains(r#"value="Docs""#) && html.contains(r#"value="Blog""#));
    assert_eq!(post_count(&db).await, before, "nothing is written");
}

#[tokio::test]
async fn an_edit_listing_no_rows_clears_the_links_and_one_omitting_them_keeps_them() {
    let db = full_db().await;
    let router = router_for_tests(db.clone());
    let client = demo_client(&router, &db).await;
    let post = create_with_links(&client, &db, "Kept").await;
    let edit = format!("/admin/posts/{}/edit", post.id);

    let resp = client.submit(&edit, "title=Kept+again").await;
    assert!(resp.status().is_redirection(), "the edit redirects");
    let kept = post_titled(&db, "Kept again").await.unwrap();
    assert_eq!(
        kept.links, post.links,
        "an edit not posting the links keeps them"
    );

    let resp = client.submit(&edit, "links=").await;
    assert!(resp.status().is_redirection(), "the edit redirects");
    let cleared = post_titled(&db, "Kept again").await.unwrap();
    assert_eq!(
        cleared.links,
        Vec::<Link>::new(),
        "no rows clears the links"
    );
}

#[tokio::test]
async fn a_row_key_the_order_or_the_item_does_not_name_is_refused() {
    let db = full_db().await;
    let router = router_for_tests(db.clone());
    let client = demo_client(&router, &db).await;
    let before = post_count(&db).await;
    for rows in [
        // Row 1 posts, but the order does not list it.
        "links=0&links.0.label=a&links.0.url=b&links.1.label=c",
        // `Link` has no `secret`.
        "links=0&links.0.label=a&links.0.url=b&links.0.secret=c",
        // Without the order, no row posts.
        "links.0.label=a&links.0.url=b",
        "links=first",
        "links=0,0&links.0.label=a&links.0.url=b",
    ] {
        let body = create_body(&db, "Refused", rows).await;
        let resp = client.submit("/admin/posts/create", &body).await;
        assert_eq!(resp.status(), 400, "{rows} is refused");
    }
    assert_eq!(post_count(&db).await, before, "nothing is written");
}

#[tokio::test]
async fn the_detail_page_shows_each_link() {
    let db = full_db().await;
    let router = router_for_tests(db.clone());
    let client = demo_client(&router, &db).await;
    let post = create_with_links(&client, &db, "Shown").await;

    let html = body_string(client.get(&format!("/admin/posts/{}", post.id)).await).await;
    let docs = html
        .find("https://docs.example")
        .expect("the first link shows");
    let blog = html
        .find("https://blog.example")
        .expect("the second link shows");
    assert!(docs < blog, "the links show in their stored order");
    assert!(html.contains("URL"), "each field shows under its label");
}
