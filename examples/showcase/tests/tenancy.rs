use http::header::COOKIE;
use showcase::models::{Author, Comment, DEMO_TENANT, Post, SIDE_TENANT, TENANTLESS_ADMIN_EMAIL};
use tablo::TenantId;
use topcoat::router::Body;

use crate::common::{
    SESSION_COOKIE, body_string, demo_client, form_body, full_db, input_value, mint_session,
    routers::router_for_tests as router, tenanted_db, tenantless_client,
};

#[tokio::test]
async fn logged_in_tenant_reaches_tenant_scoped_resources_without_headers() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    for path in ["/admin/authors", "/admin/posts"] {
        let response = client.get(path).await;
        assert_eq!(response.status(), 200, "{path}");
    }
    let html = body_string(client.get("/admin/authors").await).await;
    assert!(html.contains("Ada Author"), "{html}");
}

#[tokio::test]
async fn the_demo_admin_switches_between_their_two_blogs() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let html = body_string(client.get("/admin/authors").await).await;
    assert!(html.contains("data-tenant-switcher"), "{html}");
    assert!(html.contains("Main Blog") && html.contains("Side Project"));

    let csrf = uuid::Uuid::new_v4().to_string();
    let switched = client
        .csrf(&csrf)
        .post_form(
            "/admin/tenant",
            form_body(&[("tenant", &SIDE_TENANT.to_string()), ("csrf_token", &csrf)]),
        )
        .await;
    assert_eq!(switched.status(), 303);

    let html = body_string(client.get("/admin/authors").await).await;
    assert!(!html.contains("Ada Author"), "{html}");
}

#[tokio::test]
async fn posts_list_is_scoped_by_tenant_via_resource_query() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let resp_t1 = client.tenant(t1).get("/admin/posts").await;
    assert!(resp_t1.status().is_success());
    let html = body_string(resp_t1).await;
    assert!(html.contains("T1 Post"), "t1 should see T1 Post {}", html);
    assert!(
        !html.contains("T2 Post"),
        "t1 should not see T2 Post {}",
        html
    );

    let resp_t2 = client.tenant(t2).get("/admin/posts").await;
    assert!(resp_t2.status().is_success());
    let html = body_string(resp_t2).await;
    assert!(html.contains("T2 Post"), "t2 should see T2 Post {}", html);
    assert!(
        !html.contains("T1 Post"),
        "t2 should not see T1 Post {}",
        html
    );
}

#[tokio::test]
async fn edit_with_wrong_tenant_yields_404_via_resource_query() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db2 = db.clone();
    let t1_post = Post::filter(Post::fields().tenant_id().eq(TenantId::from(t1)))
        .first()
        .exec(&mut db2)
        .await
        .unwrap()
        .unwrap();
    let edit_url = format!("/admin/posts/{}/edit", t1_post.id);
    // Try to edit with t2 tenant -> should be 404 (not found via query)
    let resp = client.tenant(t2).get(&edit_url).await;
    assert_eq!(
        resp.status(),
        404,
        "wrong tenant should be 404, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn per_tenant_policy_deny_yields_403() {
    let (db, _, _) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let blocked = showcase::models::BLOCKED_TENANT;
    let resp = client.tenant(blocked).get("/admin/posts").await;
    assert_eq!(
        resp.status(),
        403,
        "blocked tenant should be 403, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn tenancy_via_cx_with_tenant_scopes_query_directly() {
    use showcase::app::{PostResource, admin_panel};
    use tablo::{Tenant, scoped_query};
    let (db, t1, _) = tenanted_db().await;
    let cx_t1 = admin_panel().context(&db).unwrap().with(Tenant(t1));
    let mut db_cx = tablo::db::db(&cx_t1);
    let rows = scoped_query::<PostResource>(&cx_t1)
        .unwrap()
        .exec(&mut db_cx)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "T1 Post");

    let cx_t2 = cx_t1.with(Tenant(uuid::Uuid::from_u128(2)));
    let mut db_cx2 = tablo::db::db(&cx_t2);
    let rows2 = scoped_query::<PostResource>(&cx_t2)
        .unwrap()
        .exec(&mut db_cx2)
        .await
        .unwrap();
    assert_eq!(rows2.len(), 1);
    assert_eq!(rows2[0].title, "T2 Post");
}

#[tokio::test]
async fn tenantless_requests_to_gated_resources_fail_closed() {
    // Author/Post declare a tenancy — every handler 403s
    // when the logged-in user carries no tenant, instead of leaking rows or
    // minting nil orphans.
    let db = full_db().await;
    let router = router(db.clone());
    let client = tenantless_client(&router, &db).await;

    // List without a tenant → 403 (not unscoped rows).
    let resp = client.get("/admin/posts").await;
    assert_eq!(resp.status(), 403, "tenantless list must fail closed");

    // Create without tenant → 403 and no row (not a nil-tenant orphan).
    let csrf = uuid::Uuid::new_v4().to_string();
    let mut db_q = db.clone();
    let authors = Author::all().exec(&mut db_q).await.unwrap();
    let before = showcase::models::Post::all()
        .exec(&mut db_q)
        .await
        .unwrap()
        .len();
    let resp = client
        .csrf(&csrf)
        .post_form(
            "/admin/posts/create",
            format!(
                "title=Orphan&author_id={}&cover_id=&tags=o&csrf_token={csrf}",
                authors[0].id
            ),
        )
        .await;
    assert_eq!(resp.status(), 403, "tenantless create must fail closed");
    let after = showcase::models::Post::all()
        .exec(&mut db_q)
        .await
        .unwrap()
        .len();
    assert_eq!(before, after, "no nil-tenant orphan may be minted");
    let nil_rows = showcase::models::Post::filter(
        showcase::models::Post::fields()
            .tenant_id()
            .eq(TenantId::from(uuid::Uuid::nil())),
    )
    .exec(&mut db_q)
    .await
    .unwrap();
    assert!(
        nil_rows.is_empty(),
        "seed migration must leave zero nil-tenant rows"
    );
}

#[tokio::test]
async fn tenantless_requests_to_the_comments_queue_fail_closed() {
    // Comments inherit their post's tenant through `Tenancy::via`, which gates
    // exactly as a column does: a tenantless request must not render (or
    // offer to moderate) both tenants' comments.
    let (db, _, _) = tenanted_db().await;
    let router = router(db.clone());
    let client = tenantless_client(&router, &db).await;

    let resp = client.get("/admin/comments").await;
    let status = resp.status();
    let body = body_string(resp).await;
    assert!(
        !body.contains("T1 comment") && !body.contains("T2 comment"),
        "no tenant's comment may render without a tenant: {body}"
    );
    assert_eq!(
        status, 403,
        "a tenantless comments list must fail closed, not list every tenant's queue"
    );
    let resp = client.get("/admin/comments/export").await;
    assert_eq!(resp.status(), 403, "tenantless export must fail closed");
}

#[tokio::test]
async fn create_assigns_the_logged_in_tenant() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let tenant = DEMO_TENANT;
    let csrf = uuid::Uuid::new_v4().to_string();
    let mut db_q = db.clone();
    let authors = Author::all().exec(&mut db_q).await.unwrap();
    let author_id = authors[0].id.to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            "/admin/posts/create",
            format!("title=Tenanted&author_id={author_id}&cover_id=&tags=t&csrf_token={csrf}"),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "authenticated create must redirect, got {}",
        resp.status()
    );
    let created = showcase::models::Post::filter(
        showcase::models::Post::fields()
            .title()
            .eq("Tenanted".to_string()),
    )
    .first()
    .exec(&mut db_q)
    .await
    .unwrap()
    .expect("created post");
    assert_eq!(created.tenant_id.get(), tenant);
}

#[tokio::test]
async fn x_tenant_id_header_no_longer_grants_a_tenant() {
    let db = full_db().await;
    let router = router(db.clone());
    let session = mint_session(&db, TENANTLESS_ADMIN_EMAIL).await;
    let response = router
        .handle(
            http::Request::builder()
                .uri("/admin/posts")
                .header(COOKIE, format!("{SESSION_COOKIE}={session}"))
                .header("x-tenant-id", DEMO_TENANT.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        response.status(),
        403,
        "x-tenant-id must not grant a tenant"
    );
}

#[tokio::test]
async fn bulk_delete_wrong_tenant_404s_and_deletes_nothing() {
    // The handler runs the tenant-scoped query, so a
    // cross-tenant batch comes back short and 404s.
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let t1_post = Post::filter(Post::fields().tenant_id().eq(TenantId::from(t1)))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("t1 post");
    let before = Post::filter(Post::fields().tenant_id().eq(TenantId::from(t1)))
        .exec(&mut db_q)
        .await
        .unwrap()
        .len();
    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .tenant(t2)
        .csrf(&csrf)
        .post_form(
            "/admin/posts/bulk-delete",
            format!("ids={}&confirm=1&csrf_token={csrf}", t1_post.id),
        )
        .await;
    assert_eq!(
        resp.status(),
        404,
        "cross-tenant bulk delete must 404, got {}",
        resp.status()
    );
    assert_eq!(
        Post::filter(Post::fields().tenant_id().eq(TenantId::from(t1)))
            .exec(&mut db_q)
            .await
            .unwrap()
            .len(),
        before,
        "cross-tenant batch deletes nothing"
    );
}

#[tokio::test]
async fn comments_list_is_scoped_through_parent_post() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let resp = client.tenant(t1).get("/admin/comments").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        html.contains("T1 comment"),
        "t1 should see T1 comment: {html}"
    );
    assert!(
        !html.contains("T2 comment"),
        "t1 should not see T2 comment: {html}"
    );

    let resp = client.tenant(t2).get("/admin/comments").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        html.contains("T2 comment"),
        "t2 should see T2 comment: {html}"
    );
    assert!(
        !html.contains("T1 comment"),
        "t2 should not see T1 comment: {html}"
    );
}

#[tokio::test]
async fn comments_search_is_scoped_through_parent_post() {
    let (db, t1, _) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let resp = client.tenant(t1).get("/admin/comments?q=T2+comment").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        tablo::testing::rows(&html).is_empty(),
        "t1 search for T2 comment must return zero rows: {html}"
    );

    let resp = client.tenant(t1).get("/admin/comments?q=T1+comment").await;
    let html = body_string(resp).await;
    assert!(
        !html.contains("No matches"),
        "t1 search must find its own comment: {html}"
    );
    assert!(
        html.contains("T1 comment"),
        "t1 search must still find its own comment: {html}"
    );
}

#[tokio::test]
async fn comments_export_is_scoped_through_parent_post() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let resp = client.tenant(t1).get("/admin/comments/export").await;
    assert!(resp.status().is_success());
    let csv = body_string(resp).await;
    assert!(
        csv.contains("T1 comment"),
        "t1 export must contain T1 comment, got {csv}"
    );
    assert!(
        !csv.contains("T2 comment"),
        "t1 export must not contain T2 comment, got {csv}"
    );
    let resp = client.tenant(t2).get("/admin/comments/export").await;
    let csv = body_string(resp).await;
    assert!(
        csv.contains("T2 comment") && !csv.contains("T1 comment"),
        "t2 export must be scoped, got {csv}"
    );
}

#[tokio::test]
async fn comments_edit_with_wrong_tenant_yields_404_via_resource_query() {
    let (db, _, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let t1_comment = Comment::filter(Comment::fields().body().eq("T1 comment".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("t1 comment");
    let edit_url = format!("/admin/comments/{}/edit", t1_comment.id);
    let resp = client.tenant(t2).get(&edit_url).await;
    assert_eq!(
        resp.status(),
        404,
        "wrong tenant comment edit should be 404, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn comments_query_scopes_directly_through_parent_post() {
    use showcase::app::{CommentResource, admin_panel};
    use tablo::{Tenant, scoped_query};
    let (db, t1, t2) = tenanted_db().await;
    let cx_t1 = admin_panel().context(&db).unwrap().with(Tenant(t1));
    let mut db_cx = tablo::db::db(&cx_t1);
    let rows = scoped_query::<CommentResource>(&cx_t1)
        .unwrap()
        .exec(&mut db_cx)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].body, "T1 comment");

    let cx_t2 = cx_t1.with(Tenant(t2));
    let mut db_cx2 = tablo::db::db(&cx_t2);
    let rows2 = scoped_query::<CommentResource>(&cx_t2)
        .unwrap()
        .exec(&mut db_cx2)
        .await
        .unwrap();
    assert_eq!(rows2.len(), 1);
    assert_eq!(rows2[0].body, "T2 comment");
}

#[tokio::test]
async fn export_is_scoped_by_tenant() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.tenant(t1).get("/admin/posts/export").await;
    assert!(resp.status().is_success());
    let csv = body_string(resp).await;
    assert!(
        csv.contains("T1 Post"),
        "t1 export must contain T1 Post, got {csv}"
    );
    assert!(
        !csv.contains("T2 Post"),
        "t1 export must not contain T2 Post, got {csv}"
    );
    let resp = client.tenant(t2).get("/admin/posts/export").await;
    let csv = body_string(resp).await;
    assert!(
        csv.contains("T2 Post") && !csv.contains("T1 Post"),
        "t2 export must be scoped, got {csv}"
    );
}

#[tokio::test]
async fn two_tenants_may_share_an_author_email() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let mut db_q = db.clone();
    let taken = Author::all().exec(&mut db_q).await.unwrap();
    let existing = taken
        .iter()
        .find(|a| a.tenant_id.get() == t1)
        .expect("t1 seeds an author");
    let email = existing.email.clone();

    let page = client.tenant(t2).get("/admin/authors/create").await;
    let html = body_string(page).await;
    let csrf = input_value(&html, "csrf_token").expect("create form carries csrf");

    let resp = client
        .tenant(t2)
        .csrf(&csrf)
        .post_form(
            "/admin/authors/create",
            form_body(&[
                ("name", "Cross Tenant"),
                ("email", &email),
                ("csrf_token", &csrf),
            ]),
        )
        .await;

    assert!(
        resp.status().is_redirection(),
        "two tenants may share an email, got {}, saw: {}",
        resp.status(),
        body_string(resp).await
    );
}

#[tokio::test]
async fn duplicate_email_within_one_tenant_is_reported_inline() {
    let (db, t1, _t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let mut db_q = db.clone();
    let taken = Author::all().exec(&mut db_q).await.unwrap();
    let existing = taken
        .iter()
        .find(|a| a.tenant_id.get() == t1)
        .expect("t1 seeds an author");
    let email = existing.email.clone();
    let before = Author::all().exec(&mut db_q).await.unwrap().len();

    let page = client.tenant(t1).get("/admin/authors/create").await;
    let html = body_string(page).await;
    let csrf = input_value(&html, "csrf_token").expect("create form carries csrf");

    let resp = client
        .tenant(t1)
        .csrf(&csrf)
        .post_form(
            "/admin/authors/create",
            form_body(&[
                ("name", "Same Tenant"),
                ("email", &email),
                ("csrf_token", &csrf),
            ]),
        )
        .await;

    assert!(
        resp.status().is_success(),
        "a same-tenant duplicate must re-render with an inline error, got {}",
        resp.status()
    );
    let mut db_check = db.clone();
    assert_eq!(
        Author::all().exec(&mut db_check).await.unwrap().len(),
        before,
        "a refused duplicate must not be written"
    );
}
