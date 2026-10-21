//! A post's body is a rich text field with the default toolbar, shown by a `RichTextColumn` on
//! its detail page.

use showcase::models::Post;

use crate::common::{body_string, demo_client, full_db, routers::router_for_tests as router};

/// The seeded "Hello Toasty" post.
async fn hello_post(db: &toasty::Db) -> Post {
    let mut db = db.clone();
    Post::filter(Post::fields().title().eq("Hello Toasty".to_string()))
        .first()
        .exec(&mut db)
        .await
        .unwrap()
        .expect("the seeded post")
}

async fn reload(db: &toasty::Db, post: &Post) -> Post {
    let mut db = db.clone();
    Post::get_by_id(&mut db, &post.id).await.unwrap()
}

#[tokio::test]
async fn a_posted_body_keeps_only_what_the_toolbar_produces() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let post = hello_post(&db).await;

    // `Heading1` is off the default toolbar; the rest is markup no tool produces.
    let posted = "<h1>Title</h1><p onclick=\"steal()\" style=\"position:fixed\">Hi \
                  <strong>there</strong><script>alert(1)</script> \
                  <a href=\"javascript:alert(1)\">bad</a> <a href=\"https://x.test\" \
                  target=\"_blank\">good</a></p>";
    let resp = client
        .submit(
            &format!("/admin/posts/{}/edit", post.id),
            &tablo::testing::form_body(&[("body", posted)]),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "got {} {}",
        resp.status(),
        body_string(resp).await
    );
    assert_eq!(
        reload(&db, &post).await.body,
        "Title<p>Hi <strong>there</strong> \
         <a rel=\"noopener noreferrer nofollow\">bad</a> \
         <a href=\"https://x.test\" rel=\"noopener noreferrer nofollow\">good</a></p>"
    );
}

#[tokio::test]
async fn an_empty_document_stores_the_blank_answer() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let post = hello_post(&db).await;
    assert!(!post.body.is_empty(), "the fixture must carry a body");

    let resp = client
        .submit(
            &format!("/admin/posts/{}/edit", post.id),
            &tablo::testing::form_body(&[("body", "<p></p>")]),
        )
        .await;
    assert!(resp.status().is_redirection(), "got {}", resp.status());
    assert_eq!(
        reload(&db, &post).await.body,
        "",
        "the optional body's blank answer"
    );
}

#[tokio::test]
async fn the_detail_page_cleans_a_body_written_outside_the_form() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut post = hello_post(&db).await;
    let mut db_w = db.clone();
    post.update()
        .body("<p>Kept</p><script>alert('inline')</script><img src=x onerror=\"alert('img')\">")
        .exec(&mut db_w)
        .await
        .unwrap();

    let resp = client.get(&format!("/admin/posts/{}", post.id)).await;
    assert_eq!(resp.status(), 200);
    let html = body_string(resp).await;
    assert!(
        html.contains("<p>Kept</p>"),
        "the body renders as HTML, got {html}"
    );
    assert!(
        !html.contains("alert('inline')") && !html.contains("alert('img')"),
        "the stored script never reaches the page, got {html}"
    );
}
