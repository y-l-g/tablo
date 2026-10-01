//! A post's comments on its record pages: the comments list's own table,
//! narrowed to the post.
//!
//! The seed puts every comment on "Hello Toasty" and none on "Second Post",
//! which is the fixture the property needs: the page must show *this* record's
//! comments and not the comments table. Both posts are selected by title, not
//! by "the one with comments": the showcase's tests share fixtures, and another
//! suite's rows can satisfy a property search like that.

use showcase::{
    app::router_for_tests as router,
    models::{Comment, Post},
};

use crate::common::{body_string, demo_client, form_body, full_db, input_value};

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
        other.contains("No records yet"),
        "an empty relation says so: {other}"
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

/// The relation table sorts and searches in place: the section carries the
/// live-search host and the relation shard invocation, so interactions write
/// the query signal instead of navigating.
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
        relation.contains("data-live-search"),
        "the relation hoists the live search host: {relation}"
    );
    assert!(
        relation.contains("data-live-search-transport"),
        "the host binds the query signal: {relation}"
    );
    assert!(
        relation.contains("data-query-prefix=\"comments.\""),
        "the host names the relation prefix: {relation}"
    );
    assert!(
        relation.contains("<noscript>"),
        "the GET form stays as the no-JS fallback: {relation}"
    );
    assert!(
        relation.contains("tablo-table-relation-search"),
        "the streamed region invokes the relation shard: {relation}"
    );
    assert!(
        relation.contains("comments.sort="),
        "the sort links keep the relation prefix: {relation}"
    );
    assert!(
        relation.contains("data-topcoat-on:click"),
        "the sort links write the signals instead of navigating: {relation}"
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
/// The post's detail page shows its comments read-only: no row write, no
/// bulk delete, no create link.
#[tokio::test]
async fn the_post_detail_page_shows_its_comments_read_only() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let (commented, _) = fixture_posts(&mut db_q).await;
    let html = body_string(client.get(&format!("/admin/posts/{}", commented.id)).await).await;
    let relation = &html[html.find("data-relation=").expect("the relation renders")..];
    for write in [
        "/comments/create",
        "data-row-delete-action",
        "data-bulk-form",
    ] {
        assert!(
            !relation.contains(write),
            "no {write} on the detail page: {relation}"
        );
    }
}

/// "New Comment" on the post's edit page opens the comment form with the post
/// chosen, and the created comment lands back on the edit page.
#[tokio::test]
async fn a_comment_created_from_the_post_edit_page_returns_to_it() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let (_, bare) = fixture_posts(&mut db_q).await;
    let page = format!("/admin/posts/{}/edit", bare.id);

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
    let csrf = input_value(&form, "csrf_token").expect("the form carries csrf");
    let response = client
        .csrf(&csrf)
        .post_form(
            &format!("/admin/comments/create?return={return_to}"),
            form_body(&[
                ("body", "Written from the post"),
                ("post_id", &bare.id.to_string()),
                ("csrf_token", &csrf),
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

/// A row delete confirmed from the post's edit page returns to it.
#[tokio::test]
async fn a_comment_deleted_from_the_post_edit_page_returns_to_it() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let (commented, _) = fixture_posts(&mut db_q).await;
    let page = format!("/admin/posts/{}/edit", commented.id);

    let html = body_string(client.get(&page).await).await;
    let csrf = input_value(&html, "csrf_token").expect("the page carries csrf");
    let action = html
        .split("data-row-delete-action=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("a row carries its delete action")
        .replace("&amp;", "&");
    assert!(
        action.contains("?return="),
        "the delete carries the return: {action}"
    );
    let response = client
        .csrf(&csrf)
        .post_form(
            &action,
            form_body(&[("confirm", "1"), ("csrf_token", &csrf)]),
        )
        .await;
    assert_eq!(
        response.headers().get(http::header::LOCATION).unwrap(),
        page.as_str()
    );
}

/// The edit page shows the relation below the form, outside it.
#[tokio::test]
async fn the_post_edit_page_shows_its_comments() {
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
    let html = body_string(
        client
            .get(&format!("/admin/posts/{}/edit", commented.id))
            .await,
    )
    .await;
    let relation = html
        .find("data-relation=\"comments\"")
        .expect("the relation renders");
    assert!(
        html[relation..].contains(&related[0].body),
        "the comments render in the relation: {html}"
    );
    let opened = html[..relation]
        .rfind("<form method=\"post\"")
        .expect("the edit form renders before the relation");
    assert!(
        html[opened..relation].contains("</form>"),
        "the relation sits after the edit form closes: {html}"
    );
}
