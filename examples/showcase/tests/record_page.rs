use showcase::models::Post;
use tablo::testing::{page_title, record_page_links};

use crate::common::{body_string, demo_client, full_db, routers::router_for_tests as router};

async fn a_post(db: &toasty::Db) -> Post {
    let mut db = db.clone();
    Post::all()
        .order_by(Post::fields().title().asc())
        .first()
        .exec(&mut db)
        .await
        .unwrap()
        .expect("a seeded post")
}

#[tokio::test]
async fn the_detail_page_links_a_record_page_that_renders_under_the_record() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let post = a_post(&db).await;
    let preview = format!("/admin/posts/{}/preview", post.id);

    let detail = body_string(client.get(&format!("/admin/posts/{}", post.id)).await).await;
    assert_eq!(record_page_links(&detail), [preview.as_str()]);

    let resp = client.get(&preview).await;
    assert_eq!(resp.status(), 200);
    let html = body_string(resp).await;
    assert_eq!(
        page_title(&html).as_deref(),
        Some(post.title.as_str()),
        "the page's heading names the record"
    );
}

#[tokio::test]
async fn a_record_page_its_access_refuses_answers_403_and_is_not_linked() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut post = a_post(&db).await;
    let id = post.id;
    post.update()
        .deleted_at(Some(jiff::Timestamp::now()))
        .exec(&mut db.clone())
        .await
        .unwrap();

    let detail = client.get(&format!("/admin/posts/{id}")).await;
    assert_eq!(detail.status(), 200, "a trashed post keeps its detail page");
    assert!(record_page_links(&body_string(detail).await).is_empty());
    let resp = client.get(&format!("/admin/posts/{id}/preview")).await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn a_record_page_of_an_unknown_record_answers_404() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let resp = client
        .get(&format!("/admin/posts/{}/preview", uuid::Uuid::new_v4()))
        .await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn url_record_page_names_the_page_route() {
    let db = full_db().await;
    let (_router, handle) = crate::common::routers::app_for_tests(db.clone());
    let post = a_post(&db).await;
    let cx = handle.context();

    assert_eq!(
        tablo::url::record_page::<showcase::app::PostResource, showcase::post_pages::PostPreviewPage>(
            &cx, &post
        ),
        Some(format!("/admin/posts/{}/preview", post.id))
    );
}
