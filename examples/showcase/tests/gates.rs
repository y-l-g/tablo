use http::header::LOCATION;
use showcase::models::{
    Author, BLOCKED_TENANT, Comment, DEMO_ADMIN_EMAIL, DEMO_ADMIN_PASSWORD, Post, PostStatus,
    Publication, Seo,
};
use tablo::TenantId;
use uuid::Uuid;

use crate::common::{
    SESSION_COOKIE, TestClient, comment_count, demo_client, form_body, full_db, mint_session,
    multipart_body, post_count, routers::router_for_tests as router, runtime_post,
    session_cookie_value, tenanted_db, user_count,
};

#[tokio::test]
async fn a_csrf_cookie_and_field_are_both_required() {
    let db = full_db().await;
    let router = router(db.clone());
    let session = mint_session(&db, DEMO_ADMIN_EMAIL).await;
    let client = TestClient::new(&router).cookie(SESSION_COOKIE, &session);
    let before = user_count(&db).await;

    let resp = client
        .post_form(
            "/admin/users/create",
            "name=NoToken&email=notoken%40example.com".to_string(),
        )
        .await;
    assert_eq!(
        resp.status(),
        403,
        "a POST with no CSRF cookie and no field must 403, got {}",
        resp.status()
    );
    assert_eq!(
        user_count(&db).await,
        before,
        "a refused CSRF check must create nothing"
    );
}

/// A forged CSRF pair is refused on the post routes whose rejection no other
/// suite pins — url-encoded delete and bulk delete, plus the multipart
/// create/edit upload path — and nothing they name changes.
///
/// Each route is sent twice: a field token that differs from the cookie, and no
/// `csrf_token` field at all. Both must answer 403. The regression it catches
/// is a handler that verifies CSRF after its DB work (or not at all): the
/// mismatch would then create, update or delete a row before failing.
#[tokio::test]
async fn forged_posts_answer_403_and_change_nothing() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let mut db_q = db.clone();
    let post = Post::all()
        .exec(&mut db_q)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("a seeded post");
    let author = Author::all()
        .exec(&mut db_q)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("a seeded author");

    let field = Uuid::new_v4().to_string();
    let cookie = Uuid::new_v4().to_string();
    let before = post_count(&db).await;

    for (body, label) in [
        (format!("confirm=1&csrf_token={field}"), "mismatched token"),
        ("confirm=1".to_string(), "missing token"),
    ] {
        let resp = client
            .csrf(&cookie)
            .post_form(&format!("/admin/posts/{}/delete", post.id), body)
            .await;
        assert_eq!(
            resp.status(),
            403,
            "delete {label}: a forged POST must 403, got {}",
            resp.status()
        );
    }
    assert_eq!(
        post_count(&db).await,
        before,
        "a forged delete must remove nothing"
    );

    for (body, label) in [
        (
            format!("ids={}&confirm=1&csrf_token={field}", post.id),
            "mismatched token",
        ),
        (format!("ids={}&confirm=1", post.id), "missing token"),
    ] {
        let resp = client
            .csrf(&cookie)
            .post_form("/admin/posts/bulk-delete", body)
            .await;
        assert_eq!(
            resp.status(),
            403,
            "bulk delete {label}: a forged POST must 403, got {}",
            resp.status()
        );
    }
    assert_eq!(
        post_count(&db).await,
        before,
        "a forged bulk delete must remove nothing"
    );

    let drafts = draft_count(&db).await;
    for path in [
        "/admin/posts/-/actions/publish-all-drafts",
        "/admin/-/actions/feature-tagged",
    ] {
        for (body, label) in [
            (
                format!("confirm=1&tag=draft&csrf_token={field}"),
                "mismatched token",
            ),
            ("confirm=1&tag=draft".to_string(), "missing token"),
        ] {
            let resp = client.csrf(&cookie).post_form(path, body).await;
            assert_eq!(
                resp.status(),
                403,
                "{path} {label}: a forged header action must 403, got {}",
                resp.status()
            );
        }
    }
    assert_eq!(
        draft_count(&db).await,
        drafts,
        "a forged header action must publish nothing"
    );

    let boundary = "----GateMatrixBoundary";
    let author_id = author.id.to_string();
    for (csrf, label) in [
        (Some(field.as_str()), "mismatched token"),
        (None, "missing token"),
    ] {
        let mut parts: Vec<(&str, Option<&str>, &str)> = vec![
            ("title", None, "Forged"),
            ("author_id", None, author_id.as_str()),
            ("tags", None, "forged"),
        ];
        if let Some(csrf) = csrf {
            parts.push(("csrf_token", None, csrf));
        }
        let body = multipart_body(boundary, &parts);
        let resp = client
            .csrf(&cookie)
            .post_multipart("/admin/posts/create", boundary, body.clone())
            .await;
        assert_eq!(
            resp.status(),
            403,
            "multipart create {label}: a forged POST must 403, got {}",
            resp.status()
        );

        let resp = client
            .csrf(&cookie)
            .post_multipart(&format!("/admin/posts/{}/edit", post.id), boundary, body)
            .await;
        assert_eq!(
            resp.status(),
            403,
            "multipart edit {label}: a forged POST must 403, got {}",
            resp.status()
        );
    }
    assert_eq!(
        post_count(&db).await,
        before,
        "a forged multipart create must write nothing"
    );
    let unchanged = Post::filter(Post::fields().id().eq(post.id))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the post still exists");
    assert_eq!(
        unchanged.title, post.title,
        "a forged multipart edit must not rewrite the title"
    );
}

/// A forged login POST is refused before any credential work and mints no
/// session.
///
/// The CSRF verify runs before the password is read, so a mismatched token and
/// a missing `csrf_token` field are both 403 rather than a credential verdict,
/// and a rejected attempt sets no session cookie. The regression it catches is
/// a login handler that authenticates first and verifies the token (if at all)
/// afterwards.
#[tokio::test]
async fn forged_login_answers_403_and_sets_no_session() {
    let db = full_db().await;
    let router = router(db.clone());
    let field = Uuid::new_v4().to_string();
    let cookie = Uuid::new_v4().to_string();

    for (submitted, label) in [
        (Some(field.as_str()), "mismatched token"),
        (None, "missing token"),
    ] {
        let mut pairs = vec![
            ("email", DEMO_ADMIN_EMAIL),
            ("password", DEMO_ADMIN_PASSWORD),
        ];
        if let Some(field) = submitted {
            pairs.push(("csrf_token", field));
        }
        let resp = TestClient::new(&router)
            .csrf(&cookie)
            .post_form("/admin/login", form_body(&pairs))
            .await;
        assert_eq!(
            resp.status(),
            403,
            "login {label}: a forged POST must 403, got {}",
            resp.status()
        );
        assert!(
            session_cookie_value(&resp).is_none(),
            "login {label}: a forged login must set no session cookie"
        );
    }
}

/// A forged logout POST is refused and leaves the session usable.
///
/// Both a mismatched token and a missing `csrf_token` field must 403. The
/// regression it catches is a logout that deletes the session row before
/// verifying the token, or not at all: the presented session must survive the
/// forged pair, which the follow-up authenticated GET proves.
#[tokio::test]
async fn forged_logout_answers_403_and_keeps_the_session() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let field = Uuid::new_v4().to_string();
    let cookie = Uuid::new_v4().to_string();

    for (submitted, label) in [
        (Some(field.as_str()), "mismatched token"),
        (None, "missing token"),
    ] {
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        if let Some(field) = submitted {
            pairs.push(("csrf_token", field));
        }
        let resp = client
            .csrf(&cookie)
            .post_form("/admin/logout", form_body(&pairs))
            .await;
        assert_eq!(
            resp.status(),
            403,
            "logout {label}: a forged POST must 403, got {}",
            resp.status()
        );
    }
    assert_eq!(
        client.get("/admin/posts").await.status(),
        200,
        "a forged logout must leave the session usable"
    );
}

/// Tenant B reaches none of tenant A's records: every record route 404s exactly like an unknown
/// id, a valid CSRF pair does not buy a write, and the rows are untouched.
///
/// `delete_404_for_an_unknown_id` pins the unknown-id half with a random UUID;
/// this pins the wrong-tenant half with the token the browser would actually
/// send. The edit POST 404s on its advisory, tenant-scoped load before it opens
/// a transaction (`resource_edit_post`), so the in-transaction reload that
/// repeats the scope is a second seam this test does not reach; the delete POST
/// runs its scoped load inside the transaction. The owner's own delete of the
/// same comment redirects, proving the 404s are the scope and not a dead route.
#[tokio::test]
async fn cross_tenant_requests_404_and_touch_nothing() {
    let (db, t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let comments_before = comment_count(&db).await;

    let t1_post = Post::filter(Post::fields().tenant_id().eq(TenantId::from(t1)))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("t1 post");
    let t1_comment = Comment::filter(Comment::fields().body().eq("T1 comment".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("t1 comment");
    let csrf = Uuid::new_v4().to_string();
    let foreign = client.tenant(t2).csrf(&csrf);
    let author_id = t1_post.author_id.to_string();

    for path in [
        format!("/admin/posts/{}", t1_post.id),
        format!("/admin/posts/{}/edit", t1_post.id),
        format!("/admin/comments/{}/edit", t1_comment.id),
    ] {
        let resp = foreign.get(&path).await;
        assert_eq!(resp.status(), 404, "{path}: a cross-tenant read must 404");
    }
    let resp = foreign
        .post_form(
            "/admin/posts/bulk-delete",
            form_body(&[
                ("ids", &t1_post.id.to_string()),
                ("confirm", "1"),
                ("csrf_token", &csrf),
            ]),
        )
        .await;
    assert_eq!(
        resp.status(),
        404,
        "a cross-tenant selection comes back short from the scoped fetch and 404s"
    );

    let resp = foreign
        .post_form(
            &format!("/admin/posts/{}/edit", t1_post.id),
            form_body(&[
                ("title", "Hijacked"),
                ("author_id", &author_id),
                ("csrf_token", &csrf),
            ]),
        )
        .await;
    assert_eq!(
        resp.status(),
        404,
        "a cross-tenant post edit must 404, got {}",
        resp.status()
    );

    let resp = foreign
        .post_form(
            &format!("/admin/posts/{}/delete", t1_post.id),
            form_body(&[("confirm", "1"), ("csrf_token", &csrf)]),
        )
        .await;
    assert_eq!(
        resp.status(),
        404,
        "a cross-tenant post delete must 404, got {}",
        resp.status()
    );

    let surviving_post = Post::filter(Post::fields().id().eq(t1_post.id))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the t1 post still exists");
    assert_eq!(
        surviving_post.title, t1_post.title,
        "a cross-tenant edit must not rewrite the title"
    );

    // Comments inherit their post's tenant, so the same pair 404s there too.
    let post_id = t1_post.id.to_string();
    let resp = foreign
        .post_form(
            &format!("/admin/comments/{}/edit", t1_comment.id),
            form_body(&[
                ("body", "Hijacked"),
                ("post_id", &post_id),
                ("csrf_token", &csrf),
            ]),
        )
        .await;
    assert_eq!(
        resp.status(),
        404,
        "a cross-tenant comment edit must 404, got {}",
        resp.status()
    );

    let resp = foreign
        .post_form(
            &format!("/admin/comments/{}/delete", t1_comment.id),
            form_body(&[("confirm", "1"), ("csrf_token", &csrf)]),
        )
        .await;
    assert_eq!(
        resp.status(),
        404,
        "a cross-tenant comment delete must 404, got {}",
        resp.status()
    );

    let surviving_comment = Comment::filter(Comment::fields().id().eq(t1_comment.id))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the t1 comment still exists");
    assert_eq!(
        surviving_comment.body, t1_comment.body,
        "a cross-tenant comment edit must not rewrite the body"
    );
    assert_eq!(
        comment_count(&db).await,
        comments_before,
        "a cross-tenant comment delete must remove nothing"
    );

    let resp = client
        .tenant(t1)
        .csrf(&csrf)
        .post_form(
            &format!("/admin/comments/{}/delete", t1_comment.id),
            form_body(&[("confirm", "1"), ("csrf_token", &csrf)]),
        )
        .await;
    assert_eq!(
        resp.status(),
        303,
        "the owner's comment delete must be a 303 PRG, got {}",
        resp.status()
    );
    assert_eq!(
        resp.headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/admin/comments"),
        "the owner's comment delete must redirect to the comments list"
    );
    let gone = Comment::filter(Comment::fields().id().eq(t1_comment.id))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap();
    assert!(
        gone.is_none(),
        "the owner's comment delete must remove the row"
    );
}

/// The policy gate refuses `BLOCKED_TENANT` on every read route, not only the
/// list: the list, the CSV export and the detail page each consult
/// `ViewAny`/`View`, so a refactor that dropped the policy check from
/// one of them fails here.
///
/// The detail page's check only runs against a loaded record, so the blocked
/// tenant needs a row of its own; otherwise the scoped load 404s first and the
/// policy denial would go untested.
#[tokio::test]
async fn blocked_tenant_is_refused_on_every_read_route() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let blocked = client.tenant(BLOCKED_TENANT);

    let mut db_q = db.clone();
    let blocked_author = toasty::create!(Author {
        tenant_id: TenantId::from(BLOCKED_TENANT),
        name: "Blocked Author",
        email: "blocked@example.com",
    })
    .exec(&mut db_q)
    .await
    .expect("create an author under BLOCKED_TENANT");
    let blocked_post = toasty::create!(Post {
        tenant_id: TenantId::from(BLOCKED_TENANT),
        title: "Blocked Post",
        body: "body",
        status: PostStatus::Draft,
        featured: false,
        created_at: "2024-01-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap(),
        cover_id: None,
        tags: "blocked".to_string(),
        seo: Seo {
            title: "Blocked SEO".to_string(),
            description: String::new(),
        },
        publication: Publication::Published {
            published_at: Some("2024-01-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap()),
            canonical_url: String::new(),
        },
        author_id: blocked_author.id,
    })
    .exec(&mut db_q)
    .await
    .expect("create a post under BLOCKED_TENANT");

    for path in [
        "/admin/posts".to_string(),
        "/admin/posts/export".to_string(),
        format!("/admin/posts/{}", blocked_post.id),
    ] {
        let resp = blocked.get(&path).await;
        assert_eq!(
            resp.status(),
            403,
            "{path}: a blocked tenant must be refused, got {}",
            resp.status()
        );
    }
}

/// Anonymous requests are gated on every route shape: reads redirect
/// to the login page with the validated `next`, mutations and page re-runs answer 401, and
/// nothing changes. Every route shape is listed, so a route mounted without the gate is caught
/// here.
#[tokio::test]
async fn anonymous_requests_are_gated_on_every_route() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = TestClient::new(&router);

    let mut db_q = db.clone();
    let post = Post::all()
        .exec(&mut db_q)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("a seeded post");
    let before = post_count(&db).await;

    for path in [
        "/admin/posts".to_string(),
        "/admin/posts/create".to_string(),
        format!("/admin/posts/{}", post.id),
        format!("/admin/posts/{}/edit", post.id),
        "/admin/posts/export".to_string(),
        "/admin/posts/options?q=a".to_string(),
    ] {
        let resp = client.get(&path).await;
        assert_eq!(
            resp.status(),
            307,
            "{path}: an anonymous read must redirect, got {}",
            resp.status()
        );
        let expected = format!("/admin/login?{}", form_body(&[("next", &path)]));
        assert_eq!(
            resp.headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok()),
            Some(expected.as_str()),
            "{path}: the redirect must carry the login route and the validated next"
        );
    }

    for path in [
        "/admin/posts/create".to_string(),
        format!("/admin/posts/{}/edit", post.id),
        format!("/admin/posts/{}/delete", post.id),
        "/admin/posts/bulk-delete".to_string(),
        "/admin/posts/-/actions/publish-all-drafts".to_string(),
        "/admin/-/actions/feature-tagged".to_string(),
    ] {
        let resp = client.post_form(&path, "confirm=1".to_string()).await;
        assert_eq!(
            resp.status(),
            401,
            "{path}: an anonymous mutation must answer 401, got {}",
            resp.status()
        );
    }
    assert_eq!(
        post_count(&db).await,
        before,
        "anonymous mutations must change nothing"
    );
    assert_eq!(
        runtime_post(&router, None).await.status(),
        401,
        "an anonymous page re-run must answer 401, not a redirect"
    );
}

/// The posts of every tenant still in draft.
async fn draft_count(db: &toasty::Db) -> usize {
    Post::filter(Post::fields().status().eq(PostStatus::Draft))
        .exec(&mut db.clone())
        .await
        .expect("count drafts")
        .len()
}

/// A header action loads no record, so nothing but its own scoped query keeps it in the
/// request's tenant, and nothing but the policy keeps a blocked tenant from running it.
#[tokio::test]
async fn header_actions_stay_in_their_tenant_and_policy() {
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
    Post::filter(Post::fields().id().eq(t1_post.id))
        .update()
        .status(PostStatus::Draft)
        .exec(&mut db_q)
        .await
        .expect("make the t1 post a draft");
    let csrf = Uuid::new_v4().to_string();
    let confirmed = form_body(&[("confirm", "1"), ("csrf_token", &csrf)]);

    let resp = client
        .tenant(t2)
        .csrf(&csrf)
        .post_form(
            "/admin/posts/-/actions/publish-all-drafts",
            confirmed.clone(),
        )
        .await;
    assert_eq!(resp.status(), 303, "t2 may publish its own drafts");
    assert_eq!(draft_count(&db).await, 1, "and publishes none of t1's");

    let blocked = client.tenant(BLOCKED_TENANT).csrf(&csrf);
    let resp = blocked
        .post_form("/admin/posts/-/actions/publish-all-drafts", confirmed)
        .await;
    assert_eq!(
        resp.status(),
        403,
        "the blocked tenant's policy refuses the list's action"
    );
    let resp = blocked
        .post_form(
            "/admin/-/actions/feature-tagged",
            form_body(&[("tag", "draft"), ("csrf_token", &csrf)]),
        )
        .await;
    assert_eq!(
        resp.status(),
        403,
        "the dashboard's action asks the post policy, which refuses the blocked tenant"
    );

    let resp = client
        .tenant(t1)
        .csrf(&csrf)
        .post_form(
            "/admin/-/actions/feature-tagged",
            form_body(&[("tag", "nothing-carries-this"), ("csrf_token", &csrf)]),
        )
        .await;
    assert_eq!(
        resp.status(),
        303,
        "the home page serves its action at the prefix"
    );
    assert_eq!(
        resp.headers().get(LOCATION).and_then(|v| v.to_str().ok()),
        Some("/admin"),
        "and lands back on it"
    );
}
