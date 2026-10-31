//! A table's row links: each published post's row links its public page, and a draft's none.

use showcase::models::{Post, PostStatus};
use tablo::testing::rows;

use crate::common::{body_string, demo_client, full_db, routers::router_for_tests as router};

#[tokio::test]
async fn a_published_post_links_its_public_page_and_a_draft_does_not() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get("/admin/posts").await).await;
    let posts = Post::all().exec(&mut db.clone()).await.unwrap();
    let mut statuses = Vec::new();
    for row in rows(&html) {
        let key = row.select_value.expect("a post row carries its key");
        let post = posts
            .iter()
            .find(|post| post.id.to_string() == key)
            .expect("each row is a stored post");
        let expected: Vec<String> = (post.status == PostStatus::Published)
            .then(|| format!("/blog/{}", post.id))
            .into_iter()
            .collect();
        assert_eq!(row.actions.links, expected, "{}", post.title);
        statuses.push(post.status == PostStatus::Published);
    }
    assert!(
        statuses.contains(&true) && statuses.contains(&false),
        "the demo lists both a published post and a draft: {statuses:?}"
    );
}
