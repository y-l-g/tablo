use showcase::models::{Category, Post};
use tablo::testing::{RecordNavLink, page_title, record_navigation, rows};

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

async fn a_category(db: &toasty::Db, name: &str) -> uuid::Uuid {
    let mut db = db.clone();
    Category::filter(Category::fields().name().eq(name.to_string()))
        .first()
        .exec(&mut db)
        .await
        .unwrap()
        .expect("a seeded category")
        .id
}

/// The navigation's tabs, `current` marking the one at `here`.
fn tabs(hrefs: &[&str], here: &str) -> Vec<RecordNavLink> {
    hrefs
        .iter()
        .map(|href| RecordNavLink {
            href: href.to_string(),
            current: *href == here,
        })
        .collect()
}

#[tokio::test]
async fn a_record_page_renders_under_the_record_in_its_navigation() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let post = a_post(&db).await;
    let preview = format!("/admin/posts/{}/preview", post.id);

    let view = format!("/admin/posts/{}", post.id);
    let edit = format!("{view}/edit");
    let all = [view.as_str(), edit.as_str(), preview.as_str()];

    for here in all {
        let html = body_string(client.get(here).await).await;
        assert_eq!(record_navigation(&html), tabs(&all, here), "{here}");
    }

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
async fn a_record_page_its_access_refuses_answers_403_and_leaves_the_navigation() {
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
    let view = format!("/admin/posts/{id}");
    assert_eq!(
        record_navigation(&body_string(detail).await),
        tabs(&[view.as_str()], &view),
        "a trashed post is neither edited nor previewed"
    );
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

#[tokio::test]
async fn a_relation_tab_lists_the_related_records_off_the_detail_page() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let databases = a_category(&db, "Databases").await;
    let view = format!("/admin/categories/{databases}");
    let edit = format!("{view}/edit");
    let posts = format!("{view}/posts");
    let all = [view.as_str(), edit.as_str(), posts.as_str()];

    for here in all {
        let html = body_string(client.get(here).await).await;
        assert_eq!(record_navigation(&html), tabs(&all, here), "{here}");
        let listed = rows(&html).len();
        if here == posts {
            assert_eq!(listed, 2, "the tab lists the category's two posts");
        } else {
            assert_eq!(listed, 0, "{here} lists no related row");
        }
    }
}

#[tokio::test]
async fn a_relation_tab_of_an_unknown_record_answers_404() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let resp = client
        .get(&format!("/admin/categories/{}/posts", uuid::Uuid::new_v4()))
        .await;
    assert_eq!(resp.status(), 404);
}
