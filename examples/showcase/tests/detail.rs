use showcase::models::{Author, Post, PostStatus, User};

use crate::common::{
    body_string, demo_client, full_db, routers::router_for_tests as router, tenantless_client,
};

async fn a_post_id(db: &mut toasty::Db) -> String {
    Post::all()
        .order_by(Post::fields().title().asc())
        .exec(db)
        .await
        .unwrap()
        .remove(0)
        .id
        .to_string()
}

async fn a_published_post_id(db: &mut toasty::Db) -> String {
    Post::filter(Post::fields().status().eq(PostStatus::Published))
        .order_by(Post::fields().title().asc())
        .exec(db)
        .await
        .unwrap()
        .remove(0)
        .id
        .to_string()
}

async fn an_unpublished_post_id(db: &mut toasty::Db) -> String {
    Post::filter(Post::fields().status().eq(PostStatus::Draft))
        .order_by(Post::fields().title().asc())
        .exec(db)
        .await
        .unwrap()
        .remove(0)
        .id
        .to_string()
}

fn page_heading(html: &str) -> String {
    let heading = html
        .split("<h1")
        .nth(1)
        .and_then(|rest| rest.split("</h1>").next())
        .expect("the detail page renders a page title");
    let text = heading.find('>').map_or(heading, |at| &heading[at + 1..]);
    text.trim().to_string()
}

#[tokio::test]
async fn post_detail_heading_names_the_post() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let id = a_post_id(&mut db_q).await;
    let post = Post::all()
        .filter(Post::fields().id().eq(uuid::Uuid::parse_str(&id).unwrap()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the id came from this database");

    let html = body_string(client.get(&format!("/admin/posts/{id}")).await).await;
    let heading = page_heading(&html);
    assert_eq!(heading, post.title, "the heading names the post: {html}");
    assert!(
        !heading.contains(&id),
        "the heading must not fall back to the record key: {heading}"
    );
}

#[tokio::test]
async fn post_detail_renders_the_record_read_only() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let id = a_post_id(&mut db_q).await;
    let post = Post::all()
        .filter(Post::fields().id().eq(uuid::Uuid::parse_str(&id).unwrap()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the id came from this database");
    let author = Author::filter(Author::fields().id().eq(post.author_id))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the post's author");

    let resp = client.get(&format!("/admin/posts/{id}")).await;
    assert!(
        resp.status().is_success(),
        "detail page must render, got {}",
        resp.status()
    );
    let html = body_string(resp).await;

    assert!(
        html.contains(&post.title),
        "detail page must show the title: {html}"
    );
    assert!(
        html.contains(post.status.label()),
        "detail page must show the status: {html}"
    );
    assert!(
        html.contains("Featured</div>"),
        "detail page must label the flag as featured: {html}"
    );
    let author_entry = html
        .find(">Author<")
        .unwrap_or_else(|| panic!("detail page must show the author: {html}"));
    assert!(
        html[author_entry..].contains(&author.name),
        "the author shows by name, loaded through the column's include: {html}"
    );
    assert!(
        !html.contains(&post.author_id.to_string()),
        "the author's key never shows: {html}"
    );
    assert!(
        html.contains("Back to list"),
        "detail page must offer a way back: {html}"
    );
    assert!(
        html.contains(&format!("href=\"/admin/posts/{id}/edit\"")),
        "an editable record's detail page must link its edit form: {html}"
    );
    assert!(
        html.contains("words ·") && html.contains("min read"),
        "detail page must show the computed reading stats: {html}"
    );

    let body = html
        .split("<h1")
        .nth(1)
        .and_then(|rest| rest.split("data-relation=").next())
        .expect("the detail page renders its record before its relations");
    assert!(
        !body.contains("<input") && !body.contains("<select") && !body.contains("<form"),
        "a detail page must not render form controls: {body}"
    );
    assert!(
        !body.contains("data-invalid"),
        "a stored record has nothing to be invalid about: {body}"
    );
    assert!(
        body.contains("data-slot=\"field\"") && !body.contains("data-slot=\"field-label\""),
        "the page renders values through the read-only field shape: {body}"
    );
}

#[tokio::test]
async fn an_unpublished_post_links_no_public_page() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let id = an_unpublished_post_id(&mut db_q).await;

    for path in [
        format!("/admin/posts/{id}"),
        format!("/admin/posts/{id}/edit"),
    ] {
        let html = body_string(client.get(&path).await).await;
        assert!(
            !html.contains("View public post") && !html.contains(&format!("/blog/{id}")),
            "{path} must not link a public page for a draft: {html}"
        );
    }
}

#[tokio::test]
async fn post_record_pages_link_the_public_post() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let id = a_published_post_id(&mut db_q).await;

    let edit = body_string(client.get(&format!("/admin/posts/{id}/edit")).await).await;
    assert!(
        edit.contains("View public post") && edit.contains(&format!("/blog/{id}")),
        "edit page must link the public post: {edit}"
    );

    let detail = body_string(client.get(&format!("/admin/posts/{id}")).await).await;
    assert!(
        detail.contains("View public post") && detail.contains(&format!("/blog/{id}")),
        "detail page must link the public post: {detail}"
    );
}

#[tokio::test]
async fn a_resource_without_a_view_declaration_shows_its_record_forms_fields() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let author = Author::all().exec(&mut db_q).await.unwrap().remove(0);

    let detail = body_string(client.get(&format!("/admin/authors/{}", author.id)).await).await;
    assert!(
        detail.contains(&author.name) && detail.contains(&author.email),
        "the detail page shows the record form's fields: {detail}"
    );
    assert_eq!(
        tablo::testing::input_value(&detail, "email"),
        None,
        "the detail page renders values, not controls: {detail}"
    );

    let authors = body_string(client.get("/admin/authors").await).await;
    assert_eq!(
        tablo::testing::row_actions(&authors, &author.id.to_string())
            .as_ref()
            .and_then(|actions| actions.view.clone()),
        Some(format!("/admin/authors/{}", author.id)),
        "each row links its detail page: {authors}"
    );
}

#[tokio::test]
async fn the_detail_route_does_not_shadow_create_or_edit() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db;
    let id = a_post_id(&mut db_q).await;

    let create = client.get("/admin/posts/create").await;
    assert!(
        create.status().is_success(),
        "create page must still render, got {}",
        create.status()
    );
    let create_html = body_string(create).await;
    assert!(
        create_html.contains("<form") && create_html.contains("csrf_token"),
        "the create page must still be the form: {create_html}"
    );

    let edit = client.get(&format!("/admin/posts/{id}/edit")).await;
    assert!(
        edit.status().is_success(),
        "edit page must still render, got {}",
        edit.status()
    );
    let edit_html = body_string(edit).await;
    assert!(
        edit_html.contains("<form") && edit_html.contains("csrf_token"),
        "the edit page must still be the form: {edit_html}"
    );
}

#[tokio::test]
async fn post_detail_enforces_the_tenancy() {
    let db = full_db().await;
    let router = router(db.clone());
    let mut db_q = db.clone();
    let id = a_post_id(&mut db_q).await;
    let client = tenantless_client(&router, &db).await;

    let resp = client.get(&format!("/admin/posts/{id}")).await;
    assert_eq!(
        resp.status(),
        403,
        "a tenant-gated resource must fail closed on its detail page"
    );
}

#[tokio::test]
async fn post_detail_hides_the_record_from_a_denied_tenant() {
    // The blocked tenant is refused before policy is even consulted: the
    // tenant-scoped load finds no such row, so the answer is the same 404 an
    // unknown id gets — scoping first, `View` second, which is the order
    // that keeps a 403 from confirming a record's existence across tenants.
    use showcase::models::BLOCKED_TENANT;

    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    // A real id, so the 404 is the tenant scope excluding the row rather than
    // an unknown id.
    let id = a_post_id(&mut db_q).await;

    let resp = client
        .tenant(BLOCKED_TENANT)
        .get(&format!("/admin/posts/{id}"))
        .await;
    assert_eq!(
        resp.status(),
        404,
        "a tenant the query scopes out sees the same answer as an unknown id"
    );
}

#[tokio::test]
async fn a_record_the_caller_may_not_update_offers_no_edit_action() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let ken = User::all()
        .filter(User::fields().name().eq("Ken Thompson"))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the seed has Ken");
    let resp = client.get(&format!("/admin/users/{}", ken.id)).await;
    assert!(resp.status().is_success(), "got {}", resp.status());
    let html = body_string(resp).await;
    assert!(
        !html.contains(&format!("/admin/users/{}/edit", ken.id)),
        "a record the caller may not update must not link its edit form: {html}"
    );
}
