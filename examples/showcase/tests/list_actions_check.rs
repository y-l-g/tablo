use crate::common::{
    body_string, demo_client, full_db, mount, routers::router_for_tests as router,
};

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
        let edit_targets = tablo::testing::rows(&html);
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
        if tablo::testing::row_actions(&html, &post.id.to_string())
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

/// The guide's read-only portal over the showcase's `PostResource`: the list shows the tenant's
/// drafts but offers no Publish, and a forged POST to either route publishes nothing.
#[tokio::test]
async fn a_read_only_portal_lists_drafts_and_refuses_publish() {
    use showcase::{
        app::{AuthorResource, CommentResource, PostResource},
        models::{DEMO_TENANT, Post},
    };
    use tablo::{Auth, Panel, ReadOnly, TenantId, testing::TestClient};

    let db = full_db().await;
    let router = mount(
        db.clone(),
        Panel::new("portal")
            .auth(Auth::disabled())
            .resource_with::<PostResource>(|def| def.policy(ReadOnly))
            .resource_with::<AuthorResource>(|def| def.policy(ReadOnly))
            .resource_with::<CommentResource>(|def| def.policy(ReadOnly)),
    )
    .expect("the portal mounts");
    let client = TestClient::new(&router).tenant(DEMO_TENANT);

    let list = client.get("/portal/posts").await;
    assert_eq!(list.status(), 200, "the portal opens the list");
    let html = body_string(list).await;
    let mut db_q = db.clone();
    let draft = Post::all()
        .exec(&mut db_q)
        .await
        .expect("query posts")
        .into_iter()
        .find(|post| {
            post.tenant_id == TenantId::from(DEMO_TENANT)
                && post.status == "draft"
                && tablo::testing::row_actions(&html, &post.id.to_string()).is_some()
        })
        .unwrap_or_else(|| panic!("the portal lists a draft of the demo tenant: {html}"));
    assert!(
        !html.contains("/-/actions/publish"),
        "neither a row nor the bulk bar offers Publish: {html}"
    );

    let csrf = uuid::Uuid::new_v4().to_string();
    let row = client
        .csrf(&csrf)
        .post_form(
            &format!("/portal/posts/{}/-/actions/publish", draft.id),
            format!("csrf_token={csrf}"),
        )
        .await;
    assert_eq!(row.status(), 403, "the row route refuses");
    let bulk = client
        .csrf(&csrf)
        .post_form(
            "/portal/posts/-/actions/publish",
            format!("ids={}&csrf_token={csrf}", draft.id),
        )
        .await;
    assert_eq!(bulk.status(), 403, "the bulk route refuses");
    let after = Post::get_by_id(&mut db_q, &draft.id)
        .await
        .expect("the post still exists");
    assert_eq!(after.status, "draft", "nothing was published");
}
