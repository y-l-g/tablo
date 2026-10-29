use http::header::LOCATION;
use showcase::models::{
    Author, BLOCKED_TENANT, Category, Comment, DEMO_ADMIN_EMAIL, Post, PostCategory, PostStatus,
    Publication, Seo,
};
use tablo::auth::ColumnPreference;
use uuid::Uuid;

use crate::common::{
    SESSION_COOKIE, TestClient, body_string, comment_count, demo_client, form_body, full_db,
    mint_session, multipart_body, post_count, routers::router_for_tests as router, runtime_post,
    tenanted_db, user_count,
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
            "/admin/users/-/create",
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
            .post_form("/admin/posts/-/bulk-delete", body)
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

    for (body, label) in [
        (
            format!("column=created_at&csrf_token={field}"),
            "mismatched token",
        ),
        ("column=created_at".to_string(), "missing token"),
    ] {
        let resp = client
            .csrf(&cookie)
            .post_form("/admin/posts/-/columns", body)
            .await;
        assert_eq!(
            resp.status(),
            403,
            "column picker {label}: a forged POST must 403, got {}",
            resp.status()
        );
    }
    assert!(
        ColumnPreference::all()
            .exec(&mut db_q)
            .await
            .unwrap()
            .is_empty(),
        "a forged column choice must store nothing"
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

    let category = Category::all()
        .exec(&mut db_q)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("a seeded category");
    let links = link_count(&db).await;
    let relation = format!("/admin/categories/{}/-/relations/posts", category.id);
    for path in [
        format!("{relation}/-/actions/attach"),
        format!("{relation}/-/actions/detach"),
        format!("{relation}/{}/-/actions/detach", post.id),
    ] {
        for (body, label) in [
            (
                format!("record={0}&ids={0}&csrf_token={field}", post.id),
                "mismatched token",
            ),
            (format!("record={0}&ids={0}", post.id), "missing token"),
        ] {
            let resp = client.csrf(&cookie).post_form(&path, body).await;
            assert_eq!(
                resp.status(),
                403,
                "{path} {label}: a forged link must 403, got {}",
                resp.status()
            );
        }
    }
    assert_eq!(
        link_count(&db).await,
        links,
        "a forged attach or detach must change no link"
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
            .post_multipart("/admin/posts/-/create", boundary, body.clone())
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

        let csv = format!("title,featured,author_id\nForged,false,{author_id}\n");
        let mut parts: Vec<(&str, Option<&str>, &str)> =
            vec![("file", Some("forged.csv"), csv.as_str())];
        if let Some(csrf) = csrf {
            parts.push(("csrf_token", None, csrf));
        }
        let resp = client
            .csrf(&cookie)
            .post_multipart(
                "/admin/posts/-/import",
                boundary,
                multipart_body(boundary, &parts),
            )
            .await;
        assert_eq!(
            resp.status(),
            403,
            "import {label}: a forged POST must 403, got {}",
            resp.status()
        );
    }
    assert_eq!(
        post_count(&db).await,
        before,
        "a forged multipart create or import must write nothing"
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

    let t1_post = Post::filter(Post::fields().tenant_id().eq(t1))
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
        format!("/admin/posts/{}/preview", t1_post.id),
        format!("/admin/comments/{}/edit", t1_comment.id),
    ] {
        let resp = foreign.get(&path).await;
        assert_eq!(resp.status(), 404, "{path}: a cross-tenant read must 404");
    }
    // A creatable choice's landing offers no record of another tenant. It answers as for a missing
    // key rather than 404, so it tells nothing either way.
    let landing = foreign
        .get(&format!("/admin/authors/-/picked?pick=1&key={author_id}"))
        .await;
    assert_eq!(
        tablo::testing::picked(&body_string(landing).await),
        Some(tablo::testing::Picked::Unchoosable),
        "a cross-tenant key must not become a choice"
    );
    let resp = foreign
        .post_form(
            "/admin/posts/-/bulk-delete",
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
            "/admin/posts/-/bulk-delete",
            form_body(&[
                ("matching", &format!("q={}", t1_post.title)),
                ("confirm", "1"),
                ("csrf_token", &csrf),
            ]),
        )
        .await;
    assert_eq!(
        resp.status(),
        303,
        "every match of a cross-tenant search is the tenant's own: none"
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

    // A category of tenant A, holding A's post: B neither attaches to it nor detaches from it.
    let t1_category = toasty::create!(Category {
        tenant_id: t1,
        name: "T1 Category",
    })
    .exec(&mut db_q)
    .await
    .expect("t1 category");
    toasty::create!(PostCategory {
        post_id: t1_post.id,
        category_id: t1_category.id,
    })
    .exec(&mut db_q)
    .await
    .expect("file the t1 post");
    let relation = format!("/admin/categories/{}/-/relations/posts", t1_category.id);
    let post_key = t1_post.id.to_string();
    for (path, body) in [
        (
            format!("{relation}/-/actions/attach"),
            form_body(&[("record", &post_key), ("csrf_token", &csrf)]),
        ),
        (
            format!("{relation}/-/actions/detach"),
            form_body(&[("ids", &post_key), ("csrf_token", &csrf)]),
        ),
        (
            format!("{relation}/{post_key}/-/actions/detach"),
            form_body(&[("csrf_token", &csrf)]),
        ),
    ] {
        let resp = foreign.post_form(&path, body).await;
        assert_eq!(resp.status(), 404, "{path}: a cross-tenant link must 404");
    }
    assert_eq!(
        link_count(&db).await,
        1,
        "a cross-tenant detach must keep the t1 post filed"
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
/// `ViewAny`/`View`, and the import page and its template `Create`, so a refactor that dropped
/// the policy check from one of them fails here.
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
        tenant_id: BLOCKED_TENANT,
        name: "Blocked Author",
        email: "blocked@example.com",
    })
    .exec(&mut db_q)
    .await
    .expect("create an author under BLOCKED_TENANT");
    let blocked_post = toasty::create!(Post {
        tenant_id: BLOCKED_TENANT,
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
    let blocked_category = toasty::create!(Category {
        tenant_id: BLOCKED_TENANT,
        name: "Blocked Category",
    })
    .exec(&mut db_q)
    .await
    .expect("create a category under BLOCKED_TENANT");

    for path in [
        "/admin/posts".to_string(),
        "/admin/posts/-/export".to_string(),
        "/admin/posts/-/import".to_string(),
        "/admin/posts/-/import/template".to_string(),
        format!("/admin/authors/-/picked?pick=1&key={}", blocked_author.id),
        format!("/admin/posts/{}", blocked_post.id),
        format!("/admin/posts/{}/preview", blocked_post.id),
        "/admin/posts/-/actions/tag/options?field=tags".to_string(),
        format!(
            "/admin/categories/{}/-/relations/posts/-/actions/attach/options?field=record",
            blocked_category.id
        ),
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
    let category = Category::all()
        .exec(&mut db_q)
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("a seeded category");
    let relation = format!("/admin/categories/{}/-/relations/posts", category.id);
    let before = post_count(&db).await;

    for path in [
        "/admin/posts".to_string(),
        "/admin/posts/-/create".to_string(),
        format!("/admin/posts/{}", post.id),
        format!("/admin/posts/{}/edit", post.id),
        format!("/admin/posts/{}/preview", post.id),
        "/admin/posts/-/export".to_string(),
        "/admin/posts/-/import".to_string(),
        "/admin/posts/-/import/template".to_string(),
        "/admin/posts/-/options?q=a".to_string(),
        "/admin/posts/-/picked?pick=1&key=a".to_string(),
        "/admin/-/search?q=a".to_string(),
        "/admin/posts/-/actions/tag/options?field=tags".to_string(),
        "/admin/-/actions/feature-tagged/options?field=tag".to_string(),
        format!("{relation}/-/actions/attach/options?field=record"),
        "/admin/-/error".to_string(),
        "/admin/unknown".to_string(),
        "/admin/profile".to_string(),
        "/admin/verify-email".to_string(),
        "/admin/register-tenant".to_string(),
        "/admin/tenant/profile".to_string(),
        "/admin/two-factor".to_string(),
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
        "/admin/posts/-/create".to_string(),
        "/admin/posts/-/import".to_string(),
        format!("/admin/posts/{}/edit", post.id),
        format!("/admin/posts/{}/delete", post.id),
        "/admin/posts/-/bulk-delete".to_string(),
        "/admin/posts/-/columns".to_string(),
        "/admin/posts/-/actions/publish-all-drafts".to_string(),
        "/admin/-/actions/feature-tagged".to_string(),
        format!("{relation}/-/actions/attach"),
        format!("{relation}/-/actions/detach"),
        format!("{relation}/{}/-/actions/detach", post.id),
        "/admin/profile".to_string(),
        "/admin/verify-email".to_string(),
        "/admin/register-tenant".to_string(),
        "/admin/tenant/profile".to_string(),
        "/admin/two-factor".to_string(),
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
    // A shard's path is fixed, so the refusal is all that keeps it from being probed. The
    // search shard is one the panel declares as its endpoint.
    for (path, body) in [
        ("/_topcoat/runtime/shards/tablo-live-toaster", "{}"),
        (
            "/_topcoat/runtime/shards/tablo-search",
            r#"{"args":["/admin","ada"],"signals":{}}"#,
        ),
    ] {
        let shard = http::Request::builder()
            .method(http::Method::POST)
            .uri(path)
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(topcoat::router::Body::from(body))
            .unwrap();
        assert_eq!(
            router.handle(shard).await.status(),
            401,
            "an anonymous request to {path} must answer 401, not a redirect"
        );
    }
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
    let t1_post = Post::filter(Post::fields().tenant_id().eq(t1))
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

/// Counts the join rows filing posts under categories.
async fn link_count(db: &toasty::Db) -> usize {
    let mut db = db.clone();
    PostCategory::all().exec(&mut db).await.unwrap().len()
}
