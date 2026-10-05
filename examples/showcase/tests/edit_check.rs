use http::header::LOCATION;
use showcase::models::User;

use crate::common::{
    body_string, demo_client, response_cookies, routers::router_for_tests as router, seeded_db,
    set_cookie_header,
};

#[tokio::test]
async fn edit_page_hydrates_and_updates() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let mut db_q = db.clone();
    let users = User::all().exec(&mut db_q).await.unwrap();
    let user = users.first().unwrap();
    let id = user.id.to_string();
    let edit_url = format!("/admin/users/{}/edit", id);

    let resp = client.get(&edit_url).await;
    assert!(
        resp.status().is_success(),
        "GET edit should be 200, got {}",
        resp.status()
    );
    let html = body_string(resp).await;
    assert!(
        html.contains(&user.name),
        "edit should contain hydrated name {}, got {}",
        user.name,
        html
    );
    assert!(
        html.contains(&user.email),
        "edit should contain hydrated email"
    );

    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(&edit_url, format!("name=&email=bad&csrf_token={csrf}"))
        .await;
    assert!(
        resp.status().is_success(),
        "invalid POST should re-render 200, got {}",
        resp.status()
    );
    let html = body_string(resp).await;
    assert_eq!(
        tablo_test::field_error(&html, "name").as_deref(),
        Some("Name is required"),
        "the name slot names its refusal, got {html}"
    );
    assert_eq!(
        tablo_test::field_error(&html, "email").as_deref(),
        Some("Email must be a valid email"),
        "the email slot names its refusal, got {html}"
    );
    let mut db_check = db.clone();
    let fresh = User::get_by_id(&mut db_check, &user.id).await.unwrap();
    assert_eq!(fresh.name, user.name, "should not mutate on invalid");

    let resp = client
        .csrf(&csrf)
        .post_form(
            &edit_url,
            format!("name=Updated%20Name&email=updated%40example.com&csrf_token={csrf}",),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "valid POST should redirect, got {}",
        resp.status()
    );
    let loc = resp.headers().get(LOCATION).unwrap().to_str().unwrap();
    assert!(
        loc.contains("/admin/users"),
        "redirect to list, got {}",
        loc
    );
    assert_eq!(resp.status(), 303, "a completed update is a 303 PRG");
    assert!(
        !loc.contains("notification"),
        "the toast must not ride the query, got {loc}"
    );
    let flash = set_cookie_header(&resp, "__Host-tablo_notification")
        .expect("the flash cookie is set on the redirect");
    assert!(
        flash.contains("Updated"),
        "the flash carries the action, got {flash}"
    );
    let resp2 = client.cookies(&response_cookies(&resp)).get(loc).await;
    let html2 = body_string(resp2).await;
    assert!(
        html2.contains("Updated"),
        "notification should survive, got {}",
        html2
    );
    let mut db_check2 = db.clone();
    let updated = User::get_by_id(&mut db_check2, &user.id).await.unwrap();
    assert_eq!(updated.name, "Updated Name");
    assert_eq!(updated.email, "updated@example.com");
}

#[tokio::test]
async fn edit_404_for_unknown_or_wrong_tenant() {
    // Core (`find_by_key_loads_one_row_scoped_and_404s_malformed`)
    // owns the loader unit; this pins the HTTP route. Wrong-tenant scoping
    // rides the same seam and is pinned in `tenancy_check.rs`
    // (`edit_with_wrong_tenant_yields_404_via_resource_query`).
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let fake_id = uuid::Uuid::new_v4().to_string();
    let resp = client.get(&format!("/admin/users/{}/edit", fake_id)).await;
    assert_eq!(
        resp.status(),
        404,
        "unknown id should be 404, got {}",
        resp.status()
    );
}

/// A forged edit POST must answer 403 before the advisory record lookup
/// the CSRF check runs first, so a nonexistent id cannot turn the
/// token-less 403 into a 404 existence oracle. (The 403-vs-404 distinction
/// makes this falsifiable: moving the verify back behind the load flips the
/// nonexistent-id answer to 404.)
#[tokio::test]
async fn edit_rejects_forged_post_before_probing_the_record() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let fake_id = uuid::Uuid::new_v4().to_string();
    let csrf = uuid::Uuid::new_v4().to_string();
    let cookie_mismatch = uuid::Uuid::new_v4().to_string();
    for (body, label) in [
        (format!("name=x&csrf_token={csrf}"), "mismatched token"),
        ("name=x".to_string(), "missing token"),
    ] {
        let resp = client
            .csrf(&cookie_mismatch)
            .post_form(&format!("/admin/users/{fake_id}/edit"), body)
            .await;
        assert_eq!(
            resp.status(),
            403,
            "{label}: forged edit must 403 before the advisory lookup (never 404), got {}",
            resp.status()
        );
    }
}
#[tokio::test]
async fn an_edit_writes_only_the_fields_it_posts() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let user = User::filter(User::fields().name().eq("Ada Lovelace".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("Ada is seeded");
    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            &format!("/admin/users/{}/edit", user.id),
            format!("email=kept%40example.com&csrf_token={csrf}"),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "a partial edit passes validation, got {} {}",
        resp.status(),
        body_string(resp).await
    );
    let mut db_check = db.clone();
    let fresh = User::get_by_id(&mut db_check, &user.id).await.unwrap();
    assert_eq!(fresh.email, "kept@example.com");
    assert_eq!(fresh.name, user.name, "an unposted field keeps its value");
    assert_eq!(fresh.role, user.role);
    assert_eq!(fresh.active, user.active);
    assert_eq!(fresh.age, user.age);
}

#[tokio::test]
async fn an_emptied_select_stores_its_blank_answer() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let admin = User::filter(User::fields().role().eq("admin".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("an admin is seeded");
    let csrf = uuid::Uuid::new_v4().to_string();
    let url = format!("/admin/users/{}/edit", admin.id);
    let resp = client
        .csrf(&csrf)
        .post_form(&url, format!("active=false&csrf_token={csrf}"))
        .await;
    assert!(resp.status().is_redirection());
    let resp = client
        .csrf(&csrf)
        .post_form(&url, format!("role=&active=&age=&csrf_token={csrf}"))
        .await;
    assert!(
        resp.status().is_redirection(),
        "got {} {}",
        resp.status(),
        body_string(resp).await
    );
    let mut db_check = db.clone();
    let fresh = User::get_by_id(&mut db_check, &admin.id).await.unwrap();
    assert_eq!(
        fresh.role, "member",
        "an emptied role stores the create default"
    );
    assert!(fresh.active, "an emptied active stores the create default");
    assert_eq!(fresh.age, 0, "an emptied age stores zero");
    assert_eq!(fresh.name, admin.name, "an unposted field keeps its value");
}

#[tokio::test]
async fn an_emptied_post_select_stores_its_blank_answer() {
    use showcase::models::Post;

    let db = crate::common::full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let post = Post::filter(Post::fields().title().eq("Hello Toasty".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the seeded post");
    let csrf = uuid::Uuid::new_v4().to_string();
    let url = format!("/admin/posts/{}/edit", post.id);
    let resp = client
        .csrf(&csrf)
        .post_form(
            &url,
            format!("status=published&featured=true&csrf_token={csrf}"),
        )
        .await;
    assert!(resp.status().is_redirection());
    let resp = client
        .csrf(&csrf)
        .post_form(&url, format!("status=&featured=&csrf_token={csrf}"))
        .await;
    assert!(
        resp.status().is_redirection(),
        "got {} {}",
        resp.status(),
        body_string(resp).await
    );
    let mut db_check = db.clone();
    let saved = Post::filter(Post::fields().id().eq(post.id))
        .first()
        .exec(&mut db_check)
        .await
        .unwrap()
        .expect("the post");
    assert_eq!(
        saved.status, "draft",
        "an emptied status stores the create default"
    );
    assert!(
        !saved.featured,
        "an emptied featured stores the create default"
    );
    assert_eq!(saved.title, post.title, "an unposted field keeps its value");
}

#[tokio::test]
async fn edit_sso_managed_user_is_forbidden() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let ken = showcase::models::User::filter(
        showcase::models::User::fields()
            .name()
            .eq("Ken Thompson".to_string()),
    )
    .first()
    .exec(&mut db_q)
    .await
    .unwrap()
    .expect("Ken seed");
    let resp = client.get(&format!("/admin/users/{}/edit", ken.id)).await;
    assert_eq!(
        resp.status(),
        403,
        "protected row edit page must be forbidden, got {}",
        resp.status()
    );
    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            &format!("/admin/users/{}/edit", ken.id),
            format!("name=Ken+Hacked&email=ken%40example.com&csrf_token={csrf}"),
        )
        .await;
    assert_eq!(
        resp.status(),
        403,
        "protected row edit POST must be forbidden, got {}",
        resp.status()
    );
    let mut db_check = db.clone();
    let unchanged = showcase::models::User::filter(
        showcase::models::User::fields()
            .email()
            .eq("ken@example.com".to_string()),
    )
    .first()
    .exec(&mut db_check)
    .await
    .unwrap()
    .expect("Ken unchanged");
    assert_eq!(unchanged.name, "Ken Thompson");
}

#[tokio::test]
async fn post_body_renders_as_a_textarea() {
    use showcase::models::Post;

    let db = crate::common::full_db().await;
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

    let resp = client.get(&format!("/admin/posts/{}/edit", post.id)).await;
    assert_eq!(resp.status(), 200, "GET post edit should be 200");
    let html = body_string(resp).await;

    let label = html
        .find("for=\"body\"")
        .expect("the body field must render a label");
    let close = html
        .find("</textarea>")
        .expect("the body field must render a textarea");
    let field = &html[label..close];
    assert!(
        field.contains("<textarea"),
        "the body field must be a textarea, got {field}"
    );
    assert!(
        !field.contains("<input"),
        "the body field must not be an input, got {field}"
    );
    assert!(
        field.contains(&post.body),
        "the textarea must carry the stored body, got {field}"
    );
}

#[tokio::test]
async fn post_edit_binds_and_saves_embedded_fields() {
    use showcase::models::Post;

    let db = crate::common::full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let mut db_q = db.clone();
    let post = Post::filter(Post::fields().title().eq("Hello Toasty".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the seeded post");

    let resp = client.get(&format!("/admin/posts/{}/edit", post.id)).await;
    assert_eq!(resp.status(), 200);
    let html = body_string(resp).await;
    assert_eq!(
        tablo_test::input_value(&html, "seo_title").as_deref(),
        Some(post.seo.title.as_str()),
        "the embedded leaf hydrates its flattened column, got {html}"
    );
    assert!(
        !html.contains("media_") && !html.contains("Attachment"),
        "no nested-embed demo beyond Seo may render, got {html}"
    );

    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            &format!("/admin/posts/{}/edit", post.id),
            format!(
                "title=Hello+Toasty&author_id={}&cover_id=&tags=rust&body=Body&\
                 status=published&featured=true&seo_title=Edited+SEO&seo_description=Desc&\
                 publication=2&publication_timestamp=2026-01-01T00%3A00&\
                 publication_canonical_url=https%3A%2F%2Fexample.com%2Fnew&csrf_token={csrf}",
                post.author_id
            ),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "a valid edit must redirect, got {}",
        resp.status()
    );

    let mut db_check = db.clone();
    let saved = Post::filter(Post::fields().id().eq(post.id))
        .first()
        .exec(&mut db_check)
        .await
        .unwrap()
        .expect("the post");
    assert_eq!(
        saved.seo.title, "Edited SEO",
        "the embedded struct's leaf must persist"
    );
    assert_eq!(saved.seo.description, "Desc");
}

#[tokio::test]
async fn post_edit_naming_one_embedded_leaf_keeps_the_other() {
    use showcase::models::Post;

    let db = crate::common::full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let mut db_q = db.clone();
    let post = Post::filter(Post::fields().title().eq("Hello Toasty".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the seeded post");
    assert!(
        !post.seo.description.is_empty(),
        "the fixture must carry a description to keep"
    );

    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            &format!("/admin/posts/{}/edit", post.id),
            format!("seo_title=Only+the+title&csrf_token={csrf}"),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "a partial edit must redirect, got {} {}",
        resp.status(),
        body_string(resp).await
    );

    let mut db_check = db.clone();
    let saved = Post::filter(Post::fields().id().eq(post.id))
        .first()
        .exec(&mut db_check)
        .await
        .unwrap()
        .expect("the post");
    assert_eq!(saved.seo.title, "Only the title");
    assert_eq!(
        saved.seo.description, post.seo.description,
        "the unposted leaf keeps its stored value"
    );
    assert_eq!(saved.body, post.body);
    assert_eq!(saved.status, post.status);
    assert_eq!(saved.publication, post.publication);
}

#[tokio::test]
async fn post_edit_switches_the_publication_variant_explicitly() {
    use showcase::models::{Post, Publication};

    let db = crate::common::full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let mut db_q = db.clone();
    let post = Post::filter(Post::fields().title().eq("Hello Toasty".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the seeded post");
    assert!(
        matches!(post.publication, Publication::Published { .. }),
        "the fixture must start Published"
    );

    let html = body_string(client.get(&format!("/admin/posts/{}/edit", post.id)).await).await;
    let publication_select = html
        .split_once("data-variant-select=\"publication\"")
        .unwrap_or_else(|| {
            panic!("the edit form must carry the publication variant control, got {html}")
        })
        .1;
    let publication_select = publication_select
        .split_once("</select>")
        .map(|(select, _)| select)
        .unwrap_or(publication_select);
    assert!(
        publication_select.contains("value=\"2\" selected"),
        "the stored Published variant must be the selected option, got {publication_select}"
    );
    for name in ["Scheduled", "Published", "Archived"] {
        assert!(
            publication_select.contains(&format!(">{name}<")),
            "the option must read as the variant name {name}, got {publication_select}"
        );
    }
    assert!(
        html.contains("type=\"datetime-local\""),
        "the publication timestamp must render datetime-local, got {html}"
    );

    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            &format!("/admin/posts/{}/edit", post.id),
            format!(
                "title=Hello+Toasty&author_id={}&cover_id=&tags=rust&body=Body&\
                 status=published&featured=true&seo_title=Edited+SEO&seo_description=Desc&\
                 publication=3&publication_timestamp=2026-01-01T00%3A00%3A00Z&\
                 publication_canonical_url=https%3A%2F%2Fexample.com%2Fstale&\
                 publication_reason=superseded&csrf_token={csrf}",
                post.author_id
            ),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "a valid edit must redirect, got {}",
        resp.status()
    );

    let mut db_check = db.clone();
    let saved = Post::filter(Post::fields().id().eq(post.id))
        .first()
        .exec(&mut db_check)
        .await
        .unwrap()
        .expect("the post");
    match saved.publication {
        Publication::Archived {
            archived_at,
            reason,
        } => {
            assert_eq!(
                archived_at,
                Some("2026-01-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap())
            );
            assert_eq!(reason, "superseded");
        }
        other => panic!(
            "the submitted discriminant must decide the variant, got {other:?} \
             (a filled canonical_url is not a vote)"
        ),
    }
}

#[tokio::test]
async fn post_create_keeps_the_variant_its_payload_names() {
    use showcase::models::{Post, Publication};

    let db = crate::common::full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let author = showcase::models::Author::all()
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("a seeded author");

    let html = body_string(client.get("/admin/posts/create").await).await;
    assert!(
        html.contains("name=\"publication\""),
        "the create form must carry the discriminant, got {html}"
    );

    let csrf = uuid::Uuid::new_v4().to_string();
    let author_id = author.id.to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            "/admin/posts/create",
            format!(
                "title=Created+Published&author_id={author_id}&tags=&body=Body&\
                 status=published&featured=false&cover_id=&seo_title=S&seo_description=D&\
                 publication=&publication_timestamp=2026-03-01T00%3A00%3A00Z&\
                 publication_canonical_url=https%3A%2F%2Fexample.com%2Fnew&csrf_token={csrf}"
            ),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "a valid create must redirect, got {}",
        resp.status()
    );

    let mut db_check = db.clone();
    let created = Post::filter(Post::fields().title().eq("Created Published".to_string()))
        .first()
        .exec(&mut db_check)
        .await
        .unwrap()
        .expect("the created post");
    assert_eq!(
        created.publication,
        Publication::Published {
            published_at: Some("2026-03-01T00:00:00Z".parse::<jiff::Timestamp>().unwrap()),
            canonical_url: "https://example.com/new".to_string(),
        },
        "the submitted payload names the variant when no discriminant is posted"
    );
}

#[tokio::test]
async fn post_detail_shows_computed_word_counts() {
    use showcase::{
        app::{read_minutes, word_count},
        models::Post,
    };

    assert_eq!(word_count(""), 0);
    assert_eq!(read_minutes(0), 0);
    assert_eq!(word_count("hello"), 1);
    assert_eq!(read_minutes(1), 1);
    assert_eq!(word_count(&"word ".repeat(200)), 200);
    assert_eq!(read_minutes(200), 1);
    assert_eq!(read_minutes(201), 2);

    let db = crate::common::full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let post = Post::filter(Post::fields().title().eq("Hello Toasty".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the seeded post");
    let words = word_count(&post.body);

    let html = body_string(client.get(&format!("/admin/posts/{}", post.id)).await).await;
    assert!(
        html.contains(&format!("{words} words")),
        "the detail page must show the computed word count, got {html}"
    );
    assert!(
        html.contains("min read"),
        "the detail page must show the reading time, got {html}"
    );
    let create = body_string(client.get("/admin/posts/create").await).await;
    assert!(
        !create.contains("word_count") && !create.contains("read_minutes"),
        "counts are computed, so the form carries no fields for them: {create}"
    );
}

#[tokio::test]
async fn user_age_round_trips_and_refuses_a_negative() {
    use showcase::models::User;

    let db = crate::common::seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let user = User::all().exec(&mut db_q).await.unwrap().remove(0);

    let html = body_string(client.get(&format!("/admin/users/{}/edit", user.id)).await).await;
    assert_eq!(
        tablo_test::input_value(&html, "age"),
        Some(user.age.to_string()),
        "the typed integer must hydrate its control, got {html}"
    );
    let detail = body_string(client.get(&format!("/admin/users/{}", user.id)).await).await;
    assert!(
        detail.contains(&user.age.to_string()),
        "the user detail must show the age, got {detail}"
    );

    let csrf = uuid::Uuid::new_v4().to_string();
    let before = User::filter(User::fields().id().eq(user.id))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the user")
        .age;
    let resp = client
        .csrf(&csrf)
        .post_form(
            &format!("/admin/users/{}/edit", user.id),
            format!(
                "name={}&email={}&age=-1&csrf_token={csrf}",
                user.name, user.email
            ),
        )
        .await;
    assert_eq!(
        resp.status(),
        200,
        "a negative age re-renders the form inline, got {}",
        resp.status()
    );
    let html = body_string(resp).await;
    assert_eq!(
        tablo_test::field_error(&html, "age").as_deref(),
        Some("Age must be zero or more"),
        "the age slot names the range refusal, got {html}"
    );
    let still = User::filter(User::fields().id().eq(user.id))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the user still exists");
    assert_eq!(still.age, before, "a refused edit writes nothing");
    let resp = client
        .csrf(&csrf)
        .post_form(
            &format!("/admin/users/{}/edit", user.id),
            format!(
                "name={}&email={}&age=42&csrf_token={csrf}",
                user.name, user.email
            ),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "a valid age redirects, got {}",
        resp.status()
    );
    let saved = User::filter(User::fields().id().eq(user.id))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the user still exists");
    assert_eq!(saved.age, 42);

    let resp = client
        .csrf(&csrf)
        .post_form(
            &format!("/admin/users/{}/edit", user.id),
            format!(
                "name={}&email={}&age=&csrf_token={csrf}",
                user.name, user.email
            ),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "an empty age stores zero, got {}",
        resp.status()
    );
    let cleared = User::filter(User::fields().id().eq(user.id))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the user still exists");
    assert_eq!(cleared.age, 0);
}
