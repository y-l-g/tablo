use showcase::models::{Comment, Post};

use crate::common::{
    body_string, demo_client, form_body, full_db, input_value, response_cookies,
    routers::router_for_tests as router, tenanted_db,
};

#[tokio::test]
async fn comments_list_shows_body_and_post_title() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/comments").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(html.contains("Comments</h1>"), "missing heading: {html}");
    assert!(
        html.contains("Clear write-up"),
        "missing seeded comment body: {html}"
    );
    assert!(
        html.contains("Hello Toasty"),
        "missing parent post title via include: {html}"
    );
    assert!(
        !html.contains("(unloaded)"),
        "unloaded marker leaked into list: {html}"
    );
    // The removed placeholder keeps its row: the list still shows the
    // moderation state instead of dropping the row.
    assert!(
        html.contains("[removed]"),
        "the moderation placeholder must stay visible: {html}"
    );
}

#[tokio::test]
async fn comments_list_offers_row_and_bulk_delete() {
    // The queue moderates. `CommentResource` allows
    // `DeleteAny`, so the row Delete control and the bulk bar render.
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/comments").await;
    let html = body_string(resp).await;
    assert!(
        html.contains("data-bulk-form"),
        "the moderation queue must offer bulk delete: {html}"
    );
    let rendered = tablo_test::rows(&html);
    assert!(
        !rendered.is_empty(),
        "the fixture must seed comments: {html}"
    );
    // The row control is a `?delete=<key>` link that opens the confirmation
    // dialog; the confirmed POST is what removes the row.
    assert!(
        rendered.iter().any(|row| row
            .actions
            .delete_href
            .as_deref()
            .is_some_and(|href| href.contains("delete="))),
        "the moderation queue must offer row delete: {html}"
    );
    assert!(
        rendered
            .iter()
            .any(|row| row.actions.delete_action.is_some()),
        "the row control must carry its POST target: {html}"
    );
    assert!(
        rendered.iter().any(|row| row.actions.edit.is_some()),
        "queue must keep edit links: {html}"
    );
}

#[tokio::test]
async fn comments_row_delete_removes_the_comment() {
    // The chrome above is only worth anything if the write behind it lands.
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let before = Comment::all().exec(&mut db.clone()).await.unwrap().len();
    assert!(before > 0, "the fixture must seed comments");

    let resp = client.get("/admin/comments").await;
    let html = body_string(resp).await;
    let csrf = input_value(&html, "csrf_token").expect("the list carries csrf");
    let target = tablo_test::rows(&html)
        .into_iter()
        .find_map(|row| row.actions.delete_action)
        .expect("a row delete control");

    let resp = client
        .csrf(&csrf)
        .post_form(
            &target,
            form_body(&[("confirm", "1"), ("csrf_token", &csrf)]),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "a confirmed delete must redirect, got {}",
        resp.status()
    );

    let after = Comment::all().exec(&mut db.clone()).await.unwrap().len();
    assert_eq!(after, before - 1, "the comment must be gone");
}

#[tokio::test]
async fn comments_create_form_shows_post_select() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/comments/create").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(html.contains("name=\"body\""), "missing body field: {html}");
    // The body is prose, so the form renders a textarea rather than a
    // one-line input — the same shape the post body uses.
    assert!(
        html.contains("<textarea"),
        "the comment body must render as a textarea: {html}"
    );
    assert!(
        html.contains("name=\"post_id\""),
        "missing post select: {html}"
    );
    assert!(html.contains("Hello Toasty"), "missing post option: {html}");
}

/// A Comment form's Post options load through the resource's
/// `query`, which carries no relation, so the option load selects the posts'
/// own columns and not every comment of every post.
#[tokio::test]
async fn post_options_do_not_load_every_posts_comments() {
    use showcase::app::PostResource;
    use tablo_core::{Resource, Tenant, db::db as db_handle};
    use topcoat::context::CxTestBuilder;

    let (db, t1, _t2) = tenanted_db().await;
    let cx = CxTestBuilder::new()
        .app_context(db.clone())
        .request_context(Tenant(t1))
        .build();
    let mut handle = db_handle(&cx);

    let option_row = <PostResource as Resource>::query(&cx)
        .first()
        .exec(&mut handle)
        .await
        .unwrap()
        .expect("the tenant seeds a post");
    assert!(
        option_row.comments.is_unloaded() && option_row.author.is_unloaded(),
        "the option load must not carry the resource's relations"
    );
}

#[tokio::test]
async fn comments_create_valid_redirects_and_creates() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let page = client.get("/admin/comments/create").await;
    let html = body_string(page).await;
    let csrf = input_value(&html, "csrf_token").expect("create form carries csrf");

    let mut db_q = db.clone();
    let post = Post::all().exec(&mut db_q).await.unwrap().remove(0);
    let before = Comment::all().exec(&mut db_q).await.unwrap().len();

    let resp = client
        .csrf(&csrf)
        .post_form(
            "/admin/comments/create",
            form_body(&[
                ("body", "A thoughtful follow-up"),
                ("post_id", &post.id.to_string()),
                ("csrf_token", &csrf),
            ]),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "valid create must redirect, got {}",
        resp.status()
    );
    let mut db_check = db.clone();
    let after = Comment::all().exec(&mut db_check).await.unwrap().len();
    assert_eq!(after, before + 1, "comment must be created");

    let loc = resp
        .headers()
        .get(http::header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let followed = client.cookies(&response_cookies(&resp)).get(&loc).await;
    let html = body_string(followed).await;
    assert!(html.contains("Created"), "missing created toast: {html}");
    assert!(
        html.contains("A thoughtful follow-up"),
        "new comment must render on the list: {html}"
    );
}

/// A comment cannot be pointed at another tenant's post, on create or by an
/// edit: the post field answers "Post is invalid", the form re-renders, and
/// nothing is written. The framework checks the key against the posts'
/// tenant-scoped query before the write and again inside its transaction, so
/// `CommentResource` declares no check of its own.
#[tokio::test]
async fn comments_refuse_another_tenants_post() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let foreign = Post::filter(Post::fields().tenant_id().eq(t2))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("t2 seeds one post");
    let own_comment = Comment::filter(Comment::fields().body().eq("T1 comment".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("t1 seeds one comment");
    let before = Comment::all().exec(&mut db_q).await.unwrap().len();
    let csrf = uuid::Uuid::new_v4().to_string();
    let client = client.tenant(t1).csrf(&csrf);
    let foreign_id = foreign.id.to_string();

    let created = client
        .post_form(
            "/admin/comments/create",
            form_body(&[
                ("body", "Planted"),
                ("post_id", &foreign_id),
                ("csrf_token", &csrf),
            ]),
        )
        .await;
    assert_eq!(
        created.status(),
        200,
        "a refused create re-renders the form"
    );
    let html = body_string(created).await;
    assert_eq!(
        tablo_test::field_error(&html, "post_id").as_deref(),
        Some("Post is invalid"),
        "the post field names the refusal: {html}"
    );

    let edited = client
        .post_form(
            &format!("/admin/comments/{}/edit", own_comment.id),
            form_body(&[
                ("body", "Moved"),
                ("post_id", &foreign_id),
                ("csrf_token", &csrf),
            ]),
        )
        .await;
    assert_eq!(edited.status(), 200, "a refused edit re-renders the form");
    let html = body_string(edited).await;
    assert_eq!(
        tablo_test::field_error(&html, "post_id").as_deref(),
        Some("Post is invalid"),
        "the post field names the refusal: {html}"
    );

    let mut db_check = db.clone();
    assert_eq!(
        Comment::all().exec(&mut db_check).await.unwrap().len(),
        before,
        "the refused create writes nothing"
    );
    let after = Comment::get_by_id(&mut db_check, &own_comment.id)
        .await
        .expect("the comment survives the refused edit");
    assert_eq!(
        (after.body.as_str(), after.post_id),
        ("T1 comment", own_comment.post_id),
        "the refused edit moves nothing"
    );
}
