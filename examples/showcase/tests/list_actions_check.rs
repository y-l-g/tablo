use crate::common::{body_string, demo_client, full_db, routers::router_for_tests as router};

// Every list exposes its create/edit entry points as real links.
#[tokio::test]
async fn lists_link_to_create_and_edit() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    for prefix in ["/admin/users", "/admin/authors", "/admin/posts"] {
        let resp = client.get(prefix).await;
        assert!(
            resp.status().is_success(),
            "{prefix} status {}",
            resp.status()
        );
        let html = body_string(resp).await;
        assert!(
            html.contains(&format!("href=\"{prefix}/create\"")),
            "{prefix} must link its create page, got {html}"
        );
        let edit_targets = tablo_test::rows(&html);
        assert!(
            !edit_targets.is_empty(),
            "{prefix} must render rows, got {html}"
        );
        let edits: Vec<&String> = edit_targets
            .iter()
            .filter_map(|row| row.actions.edit.as_ref())
            .collect();
        assert!(
            !edits.is_empty(),
            "{prefix} must link a row's edit page, got {html}"
        );
        for edit in &edits {
            assert!(
                edit.starts_with(&format!("{prefix}/")) && edit.ends_with("/edit"),
                "{prefix} edit link must address its own resource, got {edit}"
            );
        }
        let mut paired = 0;
        for row in &edit_targets {
            if let (Some(key), Some(edit)) = (&row.select_value, &row.actions.edit) {
                assert_eq!(
                    edit,
                    &format!("{prefix}/{key}/edit"),
                    "{prefix} edit link must name its row, got {edit}"
                );
                paired += 1;
            }
        }
        assert!(
            paired > 0,
            "{prefix} must pair at least one checkbox key with its edit link, got {html}"
        );
    }
}

/// The Publish action: a draft's row offers it, a published post's does
/// not, and the POST publishes the draft inside the framework transaction.
#[tokio::test]
async fn a_draft_post_is_published_from_its_row() {
    use showcase::models::Post;

    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get("/admin/posts").await).await;

    // The posts on this page, as the edit links name them.
    let mut db_q = db.clone();
    let mut on_page = Vec::new();
    for post in Post::all().exec(&mut db_q).await.expect("query posts") {
        let expected = format!("/admin/posts/{}/edit", post.id);
        if tablo_test::row_actions(&html, &post.id.to_string())
            .and_then(|actions| actions.edit)
            .as_deref()
            == Some(expected.as_str())
        {
            on_page.push(post);
        }
    }
    let draft = on_page
        .iter()
        .find(|p| p.status == "draft")
        .expect("the first page holds a draft");
    let published = on_page
        .iter()
        .find(|p| p.status == "published")
        .expect("the first page holds a published post");

    let publish = format!("/admin/posts/{}/-/actions/publish", draft.id);
    assert!(
        html.contains(&format!("action=\"{publish}\"")),
        "a draft's row offers Publish: {html}"
    );
    assert!(
        !html.contains(&format!("/admin/posts/{}/-/actions/publish", published.id)),
        "a published post's row does not: {html}"
    );
    assert!(
        html.contains("formaction=\"/admin/posts/-/actions/publish\""),
        "the bulk bar offers Publish for the selection: {html}"
    );

    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(&publish, format!("csrf_token={csrf}"))
        .await;
    assert!(
        resp.status().is_redirection(),
        "a committed action redirects, got {}",
        resp.status()
    );
    let after = Post::get_by_id(&mut db_q, &draft.id)
        .await
        .expect("the post still exists");
    assert_eq!(after.status, "published");
}
