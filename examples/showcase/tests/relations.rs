//! Records that point at other records: a belongs-to select and column, and a record page's table
//! of its children.

use showcase::models::{Author, Comment, Post, PostStatus};
use tablo::TenantId;

use crate::common::{
    body_string, demo_client, form_body, full_db, input_value, post_count, response_cookies,
    routers::router_for_tests as router, tenanted_db,
};

#[tokio::test]
async fn posts_list_shows_author_name() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/posts").await;
    assert!(resp.status().is_success(), "status {}", resp.status());
    let html = body_string(resp).await;
    assert!(html.contains("Hello Toasty"), "missing post title {}", html);
    assert!(html.contains("Ada Author"), "missing author name {}", html);
}

#[tokio::test]
async fn posts_create_empty_author_shows_required_error() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let before = post_count(&db).await;
    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            "/admin/posts/create",
            format!("title=Test+Post&author_id=&cover_id=&tags=a&csrf_token={csrf}",),
        )
        .await;
    let status = resp.status();
    let html = body_string(resp).await;
    assert!(
        status.is_success(),
        "empty should be 200 not redirect, got {} {}",
        status,
        html
    );
    assert_eq!(
        tablo::testing::field_error(&html, "author_id").as_deref(),
        Some("Author is required"),
        "the author slot names its refusal, got {html}"
    );
    assert_eq!(
        post_count(&db).await,
        before,
        "an invalid create must not add a post"
    );
}

#[tokio::test]
async fn posts_create_invalid_author_shows_invalid_error() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let before = post_count(&db).await;
    let csrf = uuid::Uuid::new_v4().to_string();
    let fake_id = uuid::Uuid::new_v4();
    let resp = client
        .csrf(&csrf)
        .post_form(
            "/admin/posts/create",
            format!(
                "title=Test+Post&author_id={}&cover_id=&tags=a&csrf_token={csrf}",
                fake_id
            ),
        )
        .await;
    let status = resp.status();
    let html = body_string(resp).await;
    assert!(status.is_success(), "invalid should be 200 {}", html);
    assert_eq!(
        tablo::testing::field_error(&html, "author_id").as_deref(),
        Some("Author is invalid"),
        "the author slot names its refusal, got {html}"
    );
    assert_eq!(
        post_count(&db).await,
        before,
        "an invalid create must not add a post"
    );
}
#[tokio::test]
async fn posts_edit_hydrates_author() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db2 = db.clone();
    let authors = Author::all().exec(&mut db2).await.unwrap();
    let first = &authors[0];
    // create a post via valid route to ensure edit hydrates
    let author_id = first.id.to_string();
    let _ = client
        .submit(
            "/admin/posts/create",
            &format!("title=EditMe&author_id={author_id}&cover_id=&tags=edit"),
        )
        .await;
    let mut db2 = db.clone();
    let post = Post::filter(Post::fields().title().eq("EditMe".to_string()))
        .first()
        .exec(&mut db2)
        .await
        .unwrap()
        .unwrap();
    let edit_url = format!("/admin/posts/{}/edit", post.id);
    let resp = client.get(&edit_url).await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(html.contains("EditMe"), "edit should show title {}", html);
    // The hydrated FK must match the option's canonical PK value and
    // be preselected — asserting the id appears is not enough (the option
    // value itself contains it even when nothing is selected).
    let author_option = html
        .split("<option")
        .find(|chunk| chunk.contains(&format!("value=\"{}\"", first.id)))
        .unwrap_or_else(|| panic!("edit should render an option for the stored author {html}"));
    assert!(
        author_option.contains("selected"),
        "the stored author must be preselected: {author_option}"
    );
}

#[tokio::test]
async fn posts_list_shows_comments_count_via_include() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/posts").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    // The expected counts are read from the fixture rather than
    // written as literals, so the assertion names which post gets which count
    // instead of hard-coding the seed's two numbers. The column's *cells* are
    // the observable here; "Comments" alone is the sidebar nav label present on
    // every panel page.
    let mut db_q = db.clone();
    let comments_of = async |db: &mut toasty::Db, title: &str| {
        let post = Post::filter(Post::fields().title().eq(title.to_string()))
            .first()
            .exec(db)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("the seed creates {title}"));
        Comment::filter(Comment::fields().post_id().eq(post.id))
            .exec(db)
            .await
            .unwrap()
            .len()
    };
    let hello = comments_of(&mut db_q, "Hello Toasty").await;
    let bare = comments_of(&mut db_q, "Second Post").await;
    let found = tablo::testing::rows(&html);
    let hello_row = found
        .iter()
        .find(|row| row.cells.iter().any(|cell| cell == "Hello Toasty"))
        .unwrap_or_else(|| panic!("the list shows Hello Toasty in {html}"));
    assert!(
        hello_row
            .cells
            .iter()
            .any(|cell| cell == &hello.to_string()),
        "the Comments column must show {hello} for Hello Toasty in {html}"
    );
    // Second Post paginates off the first page, so its count rides a
    // title-filtered request rather than the unfiltered list.
    let filtered = body_string(client.get("/admin/posts?q=Second+Post").await).await;
    let found = tablo::testing::rows(&filtered);
    let bare_row = found
        .iter()
        .find(|row| row.cells.iter().any(|cell| cell == "Second Post"))
        .unwrap_or_else(|| panic!("the filtered list shows Second Post in {filtered}"));
    assert!(
        bare_row.cells.iter().any(|cell| cell == &bare.to_string()),
        "the Comments column must show {bare} for Second Post in {filtered}"
    );
    // Loaded relations must never render the unloaded marker.
    assert!(
        !html.contains("(unloaded)"),
        "unloaded marker leaked into list {}",
        html
    );
}

#[tokio::test]
async fn posts_update_rechecks_author_existence() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let csrf = uuid::Uuid::new_v4().to_string();
    let mut db_q = db.clone();
    let authors = Author::all().exec(&mut db_q).await.unwrap();
    let first = &authors[0];
    let posts = Post::all().exec(&mut db_q).await.unwrap();
    let post = &posts[0];
    let edit_url = format!("/admin/posts/{}/edit", post.id);
    // Valid same-author update still redirects (symmetric double-check).
    let resp = client
        .csrf(&csrf)
        .post_form(
            &edit_url,
            format!(
                "title=Updated+Title&author_id={}&cover_id=&tags=u&csrf_token={csrf}",
                first.id
            ),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "valid update should redirect, got {}",
        resp.status()
    );
    // Bogus author is rejected, not silently written (the option check refuses it).
    let fake = uuid::Uuid::new_v4();
    let resp = client
        .submit(
            &edit_url,
            &format!("title=Bad&author_id={fake}&cover_id=&tags=u"),
        )
        .await;
    assert!(
        !resp.status().is_redirection(),
        "bogus author update must not redirect, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn posts_create_lifecycle_fields_persist() {
    // The full post form: body prose plus static lifecycle selects alongside
    // the author relationship select.
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let csrf = uuid::Uuid::new_v4().to_string();
    let mut db2 = db.clone();
    let authors = Author::all().exec(&mut db2).await.unwrap();
    let first = &authors[0];
    let author_id = first.id.to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            "/admin/posts/create",
            format!(
                "title=Lifecycle+Post&body=Full+story&status=published&featured=true&author_id={author_id}&cover_id=&tags=life&csrf_token={csrf}"
            ),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "lifecycle POST must redirect, got {}",
        resp.status()
    );
    let mut db_check = db.clone();
    let created = Post::filter(Post::fields().title().eq("Lifecycle Post".to_string()))
        .first()
        .exec(&mut db_check)
        .await
        .unwrap()
        .expect("lifecycle post");
    assert_eq!(created.body, "Full story");
    assert_eq!(created.status, PostStatus::Published);
    assert!(created.featured);
}

#[tokio::test]
async fn posts_create_omitted_lifecycle_fields_default_to_draft() {
    // Optional-with-defaults: lifecycle fields omitted from the payload
    // create a plain draft, not a validation error.
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db2 = db.clone();
    let authors = Author::all().exec(&mut db2).await.unwrap();
    let first = &authors[0];
    let author_id = first.id.to_string();
    let resp = client
        .submit(
            "/admin/posts/create",
            &format!("title=Stub+Post&author_id={author_id}&cover_id=&tags=stub"),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "stub POST must redirect, got {}",
        resp.status()
    );
    let mut db_check = db.clone();
    let created = Post::filter(Post::fields().title().eq("Stub Post".to_string()))
        .first()
        .exec(&mut db_check)
        .await
        .unwrap()
        .expect("stub post");
    assert_eq!(created.body, "");
    assert_eq!(created.status, PostStatus::Draft);
    assert!(!created.featured);
}

#[tokio::test]
async fn post_author_options_are_tenant_scoped() {
    // Relationship loads funnel through the tenant-scoped query: a foreign
    // tenant sees none of this tenant's writers.
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let resp = client.get("/admin/posts/options?field=author_id").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        html.contains("Ada Author"),
        "own-tenant options must list writers: {html}"
    );

    let foreign = client
        .tenant(uuid::Uuid::from_u128(4242))
        .get("/admin/posts/options?field=author_id")
        .await;
    assert!(foreign.status().is_success());
    let html = body_string(foreign).await;
    assert!(
        !html.contains("Ada Author"),
        "foreign tenant must not see writers: {html}"
    );
}

#[tokio::test]
async fn post_author_options_deny_blocked_tenant() {
    // Policy denial fails the options load closed: no options, no leak.
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client
        .tenant(showcase::models::BLOCKED_TENANT)
        .get("/admin/posts/options?field=author_id")
        .await;
    assert_eq!(
        resp.status(),
        403,
        "blocked tenant options must be forbidden, got {}",
        resp.status()
    );
}

// A post's comments on its record pages: the comments list's own table,
// narrowed to the post.
//
// The seed puts every comment on "Hello Toasty" and none on "Second Post",
// which is the fixture the property needs: the page must show *this* record's
// comments and not the comments table. Both posts are selected by title, not
// by "the one with comments": the showcase's tests share fixtures, and another
// suite's rows can satisfy a property search like that.

/// The commented post and a post with none, by title.
async fn fixture_posts(db: &mut toasty::Db) -> (Post, Post) {
    let commented = Post::all()
        .filter(Post::fields().title().eq("Hello Toasty".to_string()))
        .first()
        .exec(db)
        .await
        .unwrap()
        .expect("the seed creates Hello Toasty");
    let bare = Post::all()
        .filter(Post::fields().title().eq("Second Post".to_string()))
        .first()
        .exec(db)
        .await
        .unwrap()
        .expect("the seed creates Second Post");
    (commented, bare)
}

/// The post's own comments, rendered by the comments list's table.
#[tokio::test]
async fn the_post_page_lists_its_own_comments_through_the_comments_table() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let (commented, bare) = fixture_posts(&mut db_q).await;
    let related = Comment::all()
        .filter(Comment::fields().post_id().eq(commented.id))
        .exec(&mut db_q)
        .await
        .unwrap();
    assert!(
        !related.is_empty(),
        "the seed attaches comments to Hello Toasty"
    );

    let html = body_string(client.get(&format!("/admin/posts/{}", commented.id)).await).await;
    for comment in &related {
        assert!(
            html.contains(&comment.body),
            "every related row renders: missing {:?} in {html}",
            comment.body
        );
    }
    assert!(
        html.contains("data-relation=\"comments\""),
        "the relation section renders: {html}"
    );

    // The control: a post with no comments shows none of the other post's.
    let other = body_string(client.get(&format!("/admin/posts/{}", bare.id)).await).await;
    for comment in &related {
        assert!(
            !other.contains(&comment.body),
            "a post with no comments must not show another post's {:?}",
            comment.body
        );
    }
    assert!(
        tablo::testing::rows(&other).is_empty(),
        "an empty relation renders no rows: {other}"
    );
}

/// The relation's search and sort ride their own keyed parameters and stay
/// on the post page.
#[tokio::test]
async fn searching_the_relation_stays_on_the_post_page() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let (commented, _) = fixture_posts(&mut db_q).await;
    let related = Comment::all()
        .filter(Comment::fields().post_id().eq(commented.id))
        .exec(&mut db_q)
        .await
        .unwrap();
    let (kept, dropped) = (&related[0], &related[1]);
    let term = kept.body.split_whitespace().next().unwrap();
    assert!(
        !dropped.body.contains(term),
        "the fixture comments differ in their first word"
    );

    let page = format!("/admin/posts/{}", commented.id);
    let html = body_string(client.get(&format!("{page}?comments.q={term}")).await).await;
    assert!(html.contains(&kept.body), "{html}");
    assert!(
        !html.contains(&dropped.body),
        "the search narrows the relation: {html}"
    );
    assert!(
        html.contains(&format!("action=\"{page}\"")) && html.contains("name=\"comments.q\""),
        "the search form posts back to the post page: {html}"
    );
}

/// The relation table sorts and searches in place: its toolbar and sort links spell the relation's
/// prefixed parameters, and the links write the table's query instead of navigating.
#[tokio::test]
async fn the_relation_table_sorts_and_searches_in_place() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let (commented, _) = fixture_posts(&mut db_q).await;
    let page = format!("/admin/posts/{}", commented.id);

    let html = body_string(client.get(&page).await).await;
    let relation = &html[html
        .find("data-relation=\"comments\"")
        .expect("the relation renders")..];
    assert!(
        relation.contains("name=\"comments.q\""),
        "the search spells the relation prefix: {relation}"
    );
    assert!(
        relation.contains("comments.sort=") && relation.contains("data-topcoat-on:click"),
        "the sort links keep the relation prefix and write the query: {relation}"
    );

    // The prefixed sort still narrows server-side through the page URL.
    let related = Comment::all()
        .filter(Comment::fields().post_id().eq(commented.id))
        .exec(&mut db_q)
        .await
        .unwrap();
    assert!(related.len() > 1, "the fixture holds several comments");
    let sorted = body_string(
        client
            .get(&format!("{page}?comments.sort=body&comments.dir=desc"))
            .await,
    )
    .await;
    let mut bodies: Vec<&str> = related
        .iter()
        .map(|comment| {
            let at = sorted
                .find(comment.body.as_str())
                .unwrap_or_else(|| panic!("every related row renders: {}", comment.body));
            (at, comment.body.as_str())
        })
        .map(|(_, body)| body)
        .collect();
    bodies.sort_unstable();
    bodies.reverse();
    let positions: Vec<usize> = bodies
        .iter()
        .map(|body| sorted.find(body).unwrap())
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "descending sort orders the relation rows: {sorted}"
    );
}
/// The post's detail page carries its comments' writes: the create link, the row delete and the
/// bulk delete.
#[tokio::test]
async fn the_post_detail_page_carries_its_comments_writes() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let (commented, _) = fixture_posts(&mut db_q).await;
    let html = body_string(client.get(&format!("/admin/posts/{}", commented.id)).await).await;
    let relation = &html[html.find("data-relation=").expect("the relation renders")..];
    for write in [
        "/comments/create",
        "/delete?return=",
        "/comments/bulk-delete",
    ] {
        assert!(
            relation.contains(write),
            "{write} on the detail page: {relation}"
        );
    }
}

/// "New Comment" on the post's detail page opens the comment form with the post
/// chosen, and the created comment lands back on the detail page.
#[tokio::test]
async fn a_comment_created_from_the_post_page_returns_to_it() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let (_, bare) = fixture_posts(&mut db_q).await;
    let page = format!("/admin/posts/{}", bare.id);

    let html = body_string(client.get(&page).await).await;
    let return_to = page.replace('/', "%2F");
    let create = format!(
        "/admin/comments/create?post_id={}&return={return_to}",
        bare.id
    );
    assert!(
        html.contains(&create.replace('&', "&amp;")),
        "the relation links its create form: {html}"
    );

    let form = body_string(client.get(&create).await).await;
    assert!(
        form.contains(&format!("value=\"{}\" selected", bare.id)),
        "the post is preselected: {form}"
    );
    input_value(&form, "csrf_token").expect("the form carries csrf");
    let response = client
        .submit(
            &format!("/admin/comments/create?return={return_to}"),
            &form_body(&[
                ("body", "Written from the post"),
                ("post_id", &bare.id.to_string()),
            ]),
        )
        .await;
    assert_eq!(
        response.headers().get(http::header::LOCATION).unwrap(),
        page.as_str(),
        "the create returns to the post"
    );
    let html = body_string(client.get(&page).await).await;
    assert!(html.contains("Written from the post"), "{html}");
}

/// A row delete confirmed from the post's detail page returns to it.
#[tokio::test]
async fn a_comment_deleted_from_the_post_page_returns_to_it() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let (commented, _) = fixture_posts(&mut db_q).await;
    let page = format!("/admin/posts/{}", commented.id);

    let html = body_string(client.get(&page).await).await;
    input_value(&html, "csrf_token").expect("the page carries csrf");
    let action = tablo::testing::rows(&html)
        .into_iter()
        .find_map(|row| row.actions.delete_action)
        .expect("a row carries its delete action");
    assert!(
        action.contains("?return="),
        "the delete carries the return: {action}"
    );
    let response = client
        .submit(&action, &form_body(&[("confirm", "1")]))
        .await;
    assert_eq!(
        response.headers().get(http::header::LOCATION).unwrap(),
        page.as_str()
    );
}

/// The edit page renders no relation: a change to one reruns the page, which resets the fields
/// the reader has not saved.
#[tokio::test]
async fn the_post_edit_page_renders_no_relation() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let (commented, _) = fixture_posts(&mut db_q).await;
    let html = body_string(
        client
            .get(&format!("/admin/posts/{}/edit", commented.id))
            .await,
    )
    .await;
    assert!(!html.contains("data-relation="), "{html}");
}

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
        html.contains("formaction=\"/admin/comments/bulk-delete\""),
        "the moderation queue must offer bulk delete: {html}"
    );
    let rendered = tablo::testing::rows(&html);
    assert!(
        !rendered.is_empty(),
        "the fixture must seed comments: {html}"
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
    input_value(&html, "csrf_token").expect("the list carries csrf");
    let target = tablo::testing::rows(&html)
        .into_iter()
        .find_map(|row| row.actions.delete_action)
        .expect("a row delete control");

    let resp = client
        .submit(&target, &form_body(&[("confirm", "1")]))
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
    use tablo::{Resource, Tenant, db::db as db_handle};
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
    input_value(&html, "csrf_token").expect("create form carries csrf");

    let mut db_q = db.clone();
    let post = Post::all().exec(&mut db_q).await.unwrap().remove(0);
    let before = Comment::all().exec(&mut db_q).await.unwrap().len();

    let resp = client
        .submit(
            "/admin/comments/create",
            &form_body(&[
                ("body", "A thoughtful follow-up"),
                ("post_id", &post.id.to_string()),
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
    assert!(
        html.contains("data-sonner-toast"),
        "missing rendered toast: {html}"
    );
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
    let foreign = Post::filter(Post::fields().tenant_id().eq(TenantId::from(t2)))
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
        tablo::testing::field_error(&html, "post_id").as_deref(),
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
        tablo::testing::field_error(&html, "post_id").as_deref(),
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
