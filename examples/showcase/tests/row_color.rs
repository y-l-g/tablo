//! A table's row color: each row tinted by its own record.

use showcase::models::Post;
use tablo::testing::rows;

use crate::common::{body_string, demo_client, full_db, routers::router_for_tests as router};

#[tokio::test]
async fn a_featured_post_is_tinted_and_another_stays_plain() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get("/admin/posts").await).await;
    let posts = Post::all().exec(&mut db.clone()).await.unwrap();
    let rows = rows(&html);
    let mut tints = Vec::new();
    for row in rows {
        let key = row.select_value.expect("a post row carries its key");
        let post = posts
            .iter()
            .find(|post| post.id.to_string() == key)
            .expect("each row is a stored post");
        let expected = post.featured.then(|| "primary".to_string());
        assert_eq!(row.color, expected, "{}", post.title);
        tints.push(post.featured);
    }
    assert!(
        tints.contains(&true) && tints.contains(&false),
        "the demo lists both a featured post and another: {tints:?}"
    );
}
