//! The public blog: `/blog` and `/blog/{id}` answer with no session,
//! a draft is invisible on both, and the pages query the model directly.
//!
//! Every anonymous request here goes through [`TestClient::new`], which attaches
//! no cookie and no tenant — the point of the suite. The one signed-in client
//! proves a session changes nothing, and the panel request in the first test is
//! the control: the same router still gates `/admin`, so a 200 on `/blog` is the
//! blog being public rather than the gate being off.

use showcase::models::{Post, PostStatus};

use crate::common::{TestClient, body_string, full_db, routers::router_for_tests as router};

/// The one published seed post.
async fn published_post(db: &toasty::Db) -> Post {
    let mut db = db.clone();
    Post::filter(Post::fields().status().eq(PostStatus::Published))
        .first()
        .exec(&mut db)
        .await
        .expect("query the published seed post")
        .expect("the seed publishes one post")
}

/// The seeded draft, which the list must hide and the detail page must 404.
async fn draft_post(db: &toasty::Db) -> Post {
    let mut db = db.clone();
    Post::filter(Post::fields().title().eq("Second Post".to_string()))
        .first()
        .exec(&mut db)
        .await
        .expect("query the draft seed post")
        .expect("the seed creates the Second Post draft")
}

/// The path a post is served at, from the id the record carries.
fn post_path(post: &Post) -> String {
    format!("/blog/{}", post.id)
}

#[tokio::test]
async fn blog_list_and_detail_are_public() {
    let db = full_db().await;
    let router = router(db.clone());
    // No cookie, no session, no tenant: a first-time visitor.
    let client = TestClient::new(&router);
    let post = published_post(&db).await;

    let list = client.get("/blog").await;
    assert_eq!(list.status(), 200, "the list must be public");
    let list_html = body_string(list).await;

    let detail = client.get(&post_path(&post)).await;
    assert_eq!(detail.status(), 200, "the detail page must be public");
    let detail_html = body_string(detail).await;

    // The panel on the same router still redirects an anonymous visitor, so
    // the 200s above are the blog's own public routes.
    let admin = client.get("/admin/posts").await;
    assert!(
        admin.status().is_redirection(),
        "the panel stays gated, got {}",
        admin.status()
    );

    // Both pages are the blog's own document, not a nested admin shell, and
    // they carry no panel chrome — the resource loaders are panel-scoped.
    for (name, html) in [("list", &list_html), ("detail", &detail_html)] {
        assert!(
            html.contains("<!DOCTYPE html>"),
            "the {name} page must render a complete document: {html}"
        );
        assert!(
            html.contains("Published with Tablo."),
            "the {name} page must render the public layout: {html}"
        );
        assert!(
            !html.contains("data-sidebar=\"sidebar\""),
            "the {name} page must not render the admin shell: {html}"
        );
    }
}

#[tokio::test]
async fn a_draft_is_absent_from_the_list_and_404s_on_its_page() {
    let db = full_db().await;
    let router = router(db.clone());
    let draft = draft_post(&db).await;
    assert_eq!(
        draft.status,
        PostStatus::Draft,
        "the fixture must be a draft"
    );

    let client = TestClient::new(&router);

    let list = client.get("/blog").await;
    assert_eq!(list.status(), 200);
    let list_html = body_string(list).await;
    assert!(
        !list_html.contains(&draft.title),
        "a draft must not appear in the list: {list_html}"
    );
    assert!(
        !list_html.contains(&draft.body),
        "a draft's body must not appear in the list: {list_html}"
    );

    let detail = client.get(&post_path(&draft)).await;
    assert_eq!(
        detail.status(),
        404,
        "a draft's page must be not found, got {}",
        detail.status()
    );
    let detail_html = body_string(detail).await;
    assert!(
        !detail_html.contains(&draft.body),
        "a draft's body must not leak through its 404: {detail_html}"
    );
}
