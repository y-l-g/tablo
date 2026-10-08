//! Creating a record: the create page, validation, the write and its flash.

use http::header::{LOCATION, SET_COOKIE};
use showcase::models::{Author, Post, Role, User};

use crate::common::{
    body_string, demo_client, full_db, post_count, response_cookies,
    routers::router_for_tests as router, seeded_db, set_cookie_header, user_count,
};

#[tokio::test]
async fn create_page_serves_the_declared_fields() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let resp = client.get("/admin/users/create").await;
    assert!(resp.status().is_success(), "GET create should be 200");
    let html = body_string(resp).await;
    // Core (`text_input_renders_with_label_and_ac_field`)
    // owns the field detail (wrapper, Tokens, for/id, error slot); this pins
    // the HTTP wiring — the create page serves the declared fields.
    assert!(
        html.contains("<form"),
        "missing form in {}",
        &html[..html.len().min(2000)]
    );
    assert!(
        html.contains("name=\"name\"") && html.contains("name=\"email\""),
        "missing declared fields in {}",
        &html[..html.len().min(2000)]
    );
}

/// A rejected submission re-renders with the field errors, and writes nothing.
#[tokio::test]
async fn create_invalid_submission_rerenders_with_inline_errors() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let before = user_count(&db).await;
    let resp = client
        .submit("/admin/users/create", "name=&email=not-an-email")
        .await;
    let status = resp.status();
    let html = body_string(resp).await;
    assert!(
        status.is_success(),
        "invalid POST should re-render 200, not redirect, got {status}"
    );
    assert_eq!(
        tablo::testing::field_error(&html, "name").as_deref(),
        Some("Name is required"),
        "the name slot names its refusal, got {html}"
    );
    assert_eq!(
        tablo::testing::field_error(&html, "email").as_deref(),
        Some("Email must be a valid email"),
        "the email slot names its refusal, got {html}"
    );
    assert_eq!(
        user_count(&db).await,
        before,
        "an invalid create must not add a user"
    );
}

/// A valid submission is a Post/Redirect/Get with one-time flash semantics
/// (#126): 303, clean Location, the toast on the flash cookie, and the
/// follow-up response consuming it.
#[tokio::test]
async fn create_valid_redirects_with_a_one_time_flash() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client
        .submit(
            "/admin/users/create",
            "name=New%20User&email=new%40example.com",
        )
        .await;
    assert_eq!(resp.status(), 303, "a completed create is a 303");
    let loc = resp
        .headers()
        .get(LOCATION)
        .expect("missing Location")
        .to_str()
        .expect("a text Location")
        .to_string();
    assert!(
        loc.starts_with("/admin/users"),
        "redirect to list, got {loc}"
    );
    assert!(
        !loc.contains("notification"),
        "the toast must not ride the query, got {loc}"
    );
    let cookies: Vec<String> = resp
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok().map(str::to_string))
        .collect();
    assert!(
        cookies
            .iter()
            .any(|c| c.contains("__Host-tablo_notification")),
        "the flash cookie must be set on the redirect, got {cookies:?}"
    );

    // Follow the redirect, carrying whatever cookies the POST set (the flash
    // cookie included — the Location query carries no notification param).
    let resp2 = client.cookies(&response_cookies(&resp)).get(&loc).await;
    assert!(
        resp2.status().is_success(),
        "GET list after create should be 200"
    );
    // The shell consumed the one-time flash: the follow-up response clears it.
    let cleared = set_cookie_header(&resp2, "__Host-tablo_notification")
        .expect("following the redirect must consume the flash");
    assert!(
        cleared.contains("Max-Age=0") || cleared.contains("Expires=Thu, 01 Jan 1970"),
        "the flash is one-time, got {cleared}"
    );
}

#[tokio::test]
async fn create_valid_persists_the_new_user_and_toasts_it() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let before = user_count(&db).await;
    let resp = client
        .submit(
            "/admin/users/create",
            "name=New%20User&email=new%40example.com",
        )
        .await;
    assert_eq!(resp.status(), 303, "a completed create is a 303");
    let loc = resp
        .headers()
        .get(LOCATION)
        .expect("missing Location")
        .to_str()
        .expect("a text Location")
        .to_string();

    let mut db_check = db.clone();
    assert_eq!(
        user_count(&db).await,
        before + 1,
        "a valid create adds exactly one user"
    );
    let new_user = User::filter(User::fields().email().eq("new@example.com".to_string()))
        .first()
        .exec(&mut db_check)
        .await
        .unwrap();
    assert!(new_user.is_some(), "new user should exist");

    let resp2 = client.cookies(&response_cookies(&resp)).get(&loc).await;
    let html2 = body_string(resp2).await;
    assert!(
        html2.contains("data-sonner-toaster"),
        "missing the toast stack in {}",
        html2
    );
    assert!(
        html2.contains("data-sonner-toast") && html2.contains("data-type=\"success\""),
        "missing the success toast surface, got {}",
        html2
    );
}

#[tokio::test]
async fn create_post_with_unknown_keys_is_bad_request() {
    // Allow-list: role/tenant_id smuggling is a 400 at the framework
    // layer, never silently ignored.
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let before = user_count(&db).await;
    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client.csrf(&csrf).post_form("/admin/users/create", format!(
            "name=Sneaky&email=sneaky%40example.com&role=admin&tenant_id=victim&csrf_token={csrf}"
        ))
    .await;
    assert_eq!(
        resp.status(),
        400,
        "unknown POST keys must be 400, got {}",
        resp.status()
    );
    assert_eq!(
        user_count(&db).await,
        before,
        "smuggled POST must not create"
    );
}

#[tokio::test]
async fn users_create_duplicate_email_shows_taken() {
    // The declared unique() field re-renders inline instead of writing.
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let before = user_count(&db).await;
    let resp = client
        .submit(
            "/admin/users/create",
            "name=Copycat&email=ada%40example.com",
        )
        .await;
    assert!(
        resp.status().is_success(),
        "duplicate POST must re-render 200, got {}",
        resp.status()
    );
    // The inline wording is `panel::forms`'s; this pins that the duplicate
    // re-renders instead of writing.
    assert_eq!(
        user_count(&db).await,
        before,
        "duplicate POST must not create"
    );
}

#[tokio::test]
async fn users_create_static_selects_set_role_and_active() {
    // Static-options Selects: the role vocabulary and the Yes/No active pair.
    // Relationship Selects live on the post and comment forms.
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let resp = client.get("/admin/users/create").await;
    let html = body_string(resp).await;
    assert!(html.contains("Profile"), "missing profile section: {html}");
    assert!(
        html.contains("name=\"role\""),
        "missing role select: {html}"
    );
    assert!(
        html.contains("name=\"active\""),
        "missing active select: {html}"
    );

    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            "/admin/users/create",
            format!(
                "name=New+Admin&email=newadmin%40example.com&role=admin&active=false&csrf_token={csrf}"
            ),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "valid static-select POST must redirect, got {}",
        resp.status()
    );
    let mut db_check = db.clone();
    let created = User::filter(
        User::fields()
            .email()
            .eq("newadmin@example.com".to_string()),
    )
    .first()
    .exec(&mut db_check)
    .await
    .unwrap()
    .expect("created user");
    assert_eq!(created.role, Role::Admin);
    assert!(!created.active);
}

#[tokio::test]
async fn posts_create_invalid_shows_errors() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let before = post_count(&db).await;
    let mut db2 = db.clone();
    let authors = Author::all().exec(&mut db2).await.unwrap();
    let first = &authors[0];
    // Missing title (required). The optional Tags field is empty, which is
    // its blank answer — not an error.
    let resp = client
        .submit(
            "/admin/posts/create",
            &format!("title=&author_id={}&tags=", first.id),
        )
        .await;
    let status = resp.status();
    let html = body_string(resp).await;
    assert!(
        status.is_success(),
        "invalid should be 200, got {status} {html}"
    );
    assert_eq!(
        tablo::testing::field_error(&html, "title").as_deref(),
        Some("Title is required"),
        "the title slot names its refusal, got {html}"
    );
    assert_eq!(
        post_count(&db).await,
        before,
        "an invalid create must not add a post"
    );
}

#[tokio::test]
async fn posts_create_valid_creates() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let csrf = uuid::Uuid::new_v4().to_string();
    let mut db2 = db.clone();
    let authors = Author::all().exec(&mut db2).await.unwrap();
    let first = &authors[0];
    let before = Post::all().exec(&mut db2).await.unwrap().len();
    let author_id = first.id.to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            "/admin/posts/create",
            format!(
                "title=Valid+With+Tags&author_id={author_id}&cover_id=&tags=valid%2Ctags&csrf_token={csrf}"
            ),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "valid should redirect, got {} ",
        resp.status()
    );
    let mut db2 = db.clone();
    let after = Post::all().exec(&mut db2).await.unwrap().len();
    assert_eq!(after, before + 1);
    let created = Post::filter(Post::fields().title().eq("Valid With Tags".to_string()))
        .first()
        .exec(&mut db2)
        .await
        .unwrap();
    assert!(created.is_some());
    let post = created.unwrap();
    assert_eq!(post.tags, "valid,tags");
    assert_eq!(post.cover_id, None);
}

/// `PostForm` declares `tags` `#[form(optional)]`, so an empty Tags field submits cleanly and
/// stores the empty string.
#[tokio::test]
async fn posts_create_with_empty_optional_tags_submits() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db2 = db.clone();
    let authors = Author::all().exec(&mut db2).await.unwrap();
    let before = Post::all().exec(&mut db2).await.unwrap().len();

    let author_id = authors[0].id.to_string();
    let resp = client
        .submit(
            "/admin/posts/create",
            &format!("title=No+Tags&author_id={author_id}&cover_id=&tags="),
        )
        .await;
    let status = resp.status();
    assert!(
        status.is_redirection(),
        "an empty optional Tags field must not fail the submit, got {status} {}",
        body_string(resp).await
    );
    let mut db2 = db.clone();
    assert_eq!(
        Post::all().exec(&mut db2).await.unwrap().len(),
        before + 1,
        "the post is created"
    );
    let created = Post::filter(Post::fields().title().eq("No Tags".to_string()))
        .first()
        .exec(&mut db2)
        .await
        .unwrap()
        .expect("created post");
    assert_eq!(created.tags, "", "the emptied field stores empty");
}

#[tokio::test]
async fn users_create_form_stays_urlencoded() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/users/create").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        !html.contains("multipart/form-data"),
        "plain form must stay urlencoded, got {}",
        &html[..html.len().min(2000)]
    );
}

#[tokio::test]
async fn posts_create_form_stays_urlencoded_without_uploads() {
    // One media source: the post form picks a library row instead of uploading
    // bytes, so it stays urlencoded like every other plain form.
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/posts/create").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        !html.contains("multipart/form-data"),
        "the post form carries no file input, got {}",
        &html[..html.len().min(2000)]
    );
}

#[tokio::test]
async fn posts_author_select_is_searchable() {
    // The relationship select carries the client-side filter hook.
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let resp = client.get("/admin/posts/create").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    assert!(
        html.contains("data-options-filter"),
        "author select must render the filter hook, got {html}"
    );
    // Hiding the native select is the script's job, so the markup keeps
    // both controls. The select stays the submitted value carrier, and
    // `partsOf` keeps resolving it as a descendant of the filterable field.
    let author_select = html
        .match_indices("<select")
        .map(|(start, _)| opening_tag_at(&html, start))
        .find(|tag| tag.contains("name=\"author_id\""))
        .expect("the author select stays in the markup as the submitted value carrier");
    assert!(
        author_select.contains("id=\"author_id\""),
        "the author select keeps its field id, got {author_select}"
    );
}

/// The opening tag that starts at `start`, up to its unquoted `>`.
fn opening_tag_at(html: &str, start: usize) -> &str {
    let mut quoted = false;
    for (offset, byte) in html[start..].bytes().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b'>' if !quoted => return &html[start..start + offset],
            _ => {}
        }
    }
    panic!("unterminated tag at byte {start}");
}
