//! The post list's featured switch: flipping it saves the post and lands back on the list.

use showcase::models::Post;
use tablo::testing::rows;

use crate::common::{body_string, demo_client, full_db, routers::router_for_tests as router};

#[tokio::test]
async fn flipping_a_posts_switch_features_it_and_lands_back_on_the_list() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get("/admin/posts").await).await;
    let posts = Post::all().exec(&mut db.clone()).await.unwrap();
    let (post, edit) = rows(&html)
        .into_iter()
        .find_map(|row| {
            let key = row.select_value?;
            let post = posts.iter().find(|post| post.id.to_string() == key)?;
            let edit = row
                .edits
                .into_iter()
                .find(|edit| edit.column == "featured")?;
            (!post.featured).then(|| (post.clone(), edit))
        })
        .expect("the demo lists a post no one featured, with its switch");
    assert_eq!(edit.value.as_deref(), Some("false"));

    let response = client
        .submit(&edit.action, "featured=false&featured=true")
        .await;
    assert!(response.status().is_redirection(), "{}", response.status());
    let location = response.headers()["location"].to_str().unwrap();
    assert!(location.starts_with("/admin/posts"), "{location}");
    let saved = Post::get_by_id(&mut db.clone(), &post.id).await.unwrap();
    assert!(saved.featured);
    assert_eq!(
        (saved.title, saved.status, saved.author_id),
        (post.title, post.status, post.author_id),
        "the save keeps what the switch does not post"
    );
}
