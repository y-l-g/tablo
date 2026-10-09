//! A many-to-many relation: a post's categories, chosen on its form and stored as join rows, and a
//! category's posts, attached and detached from the category's page.

use showcase::models::{Category, DEMO_TENANT, Post, PostCategory, SIDE_TENANT};
use tablo::{
    TenantId,
    testing::{field_error, rows},
};
use toasty::Db;
use uuid::Uuid;

use crate::common::{
    body_string, demo_client, form_body, full_db, routers::router_for_tests as router,
};

/// The seeded "Hello Toasty", filed under Engineering and Databases.
const HELLO: Uuid = Uuid::from_u128(0);
/// The seeded "Second Post", filed under nothing.
const SECOND: Uuid = Uuid::from_u128(1);
/// The seeded "Reviewing Query Plans", filed under Databases.
const PLANS: Uuid = Uuid::from_u128(4);

/// The demo tenant's category `name`.
async fn category(db: &Db, name: &str) -> Uuid {
    let mut db = db.clone();
    Category::filter(Category::fields().name().eq(name.to_string()))
        .first()
        .exec(&mut db)
        .await
        .unwrap()
        .unwrap_or_else(|| panic!("a seeded category {name}"))
        .id
}

/// The categories `post` is filed under, by the join rows that file it.
async fn filed(db: &Db, post: Uuid) -> Vec<Uuid> {
    let mut db = db.clone();
    let mut categories: Vec<Uuid> = PostCategory::filter(PostCategory::fields().post_id().eq(post))
        .exec(&mut db)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.category_id)
        .collect();
    categories.sort();
    categories
}

fn sorted(mut ids: Vec<Uuid>) -> Vec<Uuid> {
    ids.sort();
    ids
}

/// Whether the `categories` checkbox of `category` renders checked, or `None` when the form
/// offers no such box.
fn checked(html: &str, category: Uuid) -> Option<bool> {
    let value = format!("value=\"{category}\"");
    html.split("<input")
        .map(|tag| &tag[..tag.find('>').unwrap_or(tag.len())])
        .find(|tag| tag.contains("name=\"categories\"") && tag.contains(&value))
        .map(|tag| tag.contains(" checked=\"\""))
}

#[tokio::test]
async fn creating_a_post_files_it_under_the_chosen_categories() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let (engineering, process) = (
        category(&db, "Engineering").await,
        category(&db, "Process").await,
    );
    let author = {
        let mut db = db.clone();
        Post::filter(Post::fields().id().eq(HELLO))
            .first()
            .exec(&mut db)
            .await
            .unwrap()
            .unwrap()
            .author_id
            .to_string()
    };
    // The browser posts the hidden blank first, then each checked box.
    let body = form_body(&[
        ("title", "Filed"),
        ("author_id", &author),
        ("categories", ""),
        ("categories", &engineering.to_string()),
        ("categories", &process.to_string()),
    ]);
    let resp = client.submit("/admin/posts/create", &body).await;
    assert_eq!(resp.status(), 303, "{}", body_string(resp).await);
    let mut db_q = db.clone();
    let post = Post::filter(Post::fields().title().eq("Filed".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("the created post");
    assert_eq!(
        filed(&db, post.id).await,
        sorted(vec![engineering, process])
    );
}

#[tokio::test]
async fn the_edit_form_checks_the_categories_the_post_is_filed_under() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get(&format!("/admin/posts/{HELLO}/edit")).await).await;
    for (name, expected) in [
        ("Engineering", true),
        ("Databases", true),
        ("Process", false),
    ] {
        assert_eq!(
            checked(&html, category(&db, name).await),
            Some(expected),
            "{name}: {html}"
        );
    }
}

#[tokio::test]
async fn an_edit_files_the_post_under_exactly_the_categories_it_posts() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let (databases, process) = (
        category(&db, "Databases").await,
        category(&db, "Process").await,
    );
    let url = format!("/admin/posts/{HELLO}/edit");
    let body = form_body(&[
        ("categories", ""),
        ("categories", &databases.to_string()),
        ("categories", &process.to_string()),
    ]);
    let resp = client.submit(&url, &body).await;
    assert_eq!(resp.status(), 303, "{}", body_string(resp).await);
    assert_eq!(filed(&db, HELLO).await, sorted(vec![databases, process]));

    // Posting the hidden blank alone is a form with no box checked.
    let resp = client.submit(&url, &form_body(&[("categories", "")])).await;
    assert_eq!(resp.status(), 303);
    assert!(filed(&db, HELLO).await.is_empty());
}

#[tokio::test]
async fn an_edit_that_does_not_post_the_categories_keeps_them() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let before = filed(&db, HELLO).await;
    assert_eq!(before.len(), 2, "the seed files the post twice");
    let resp = client
        .submit(
            &format!("/admin/posts/{HELLO}/edit"),
            &form_body(&[("title", "Renamed")]),
        )
        .await;
    assert_eq!(resp.status(), 303);
    assert_eq!(filed(&db, HELLO).await, before);
}

#[tokio::test]
async fn another_tenants_category_is_refused_and_nothing_is_written() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let foreign = toasty::create!(Category {
        tenant_id: TenantId::from(SIDE_TENANT),
        name: "Foreign",
    })
    .exec(&mut db_q)
    .await
    .unwrap();
    let before = filed(&db, HELLO).await;
    let resp = client
        .submit(
            &format!("/admin/posts/{HELLO}/edit"),
            &form_body(&[("categories", &foreign.id.to_string())]),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let html = body_string(resp).await;
    assert_eq!(
        field_error(&html, "categories").as_deref(),
        Some("Categories is invalid")
    );
    assert_eq!(filed(&db, HELLO).await, before);
}

#[tokio::test]
async fn a_category_page_lists_the_posts_filed_under_it() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let databases = category(&db, "Databases").await;
    let html = body_string(client.get(&format!("/admin/categories/{databases}")).await).await;
    let mut listed: Vec<String> = rows(&html)
        .into_iter()
        .filter_map(|row| row.select_value)
        .collect();
    listed.sort();
    assert_eq!(listed, [HELLO.to_string(), PLANS.to_string()]);
    let relation = format!("/admin/categories/{databases}/-/relations/posts");
    assert!(
        html.contains(&format!("{relation}/-/actions/attach")),
        "the header attaches: {html}"
    );
    assert!(
        html.contains(&format!("{relation}/{HELLO}/-/actions/detach")),
        "each row detaches: {html}"
    );
}

#[tokio::test]
async fn attaching_files_the_chosen_post_once() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let databases = category(&db, "Databases").await;
    let attach = format!("/admin/categories/{databases}/-/relations/posts/-/actions/attach");
    for post in [SECOND, SECOND, HELLO] {
        let resp = client
            .submit(&attach, &form_body(&[("record", &post.to_string())]))
            .await;
        assert_eq!(resp.status(), 303, "{}", body_string(resp).await);
    }
    assert_eq!(filed(&db, SECOND).await, vec![databases]);
    assert_eq!(
        filed(&db, HELLO).await,
        sorted(vec![category(&db, "Engineering").await, databases]),
        "attaching a filed post files it once"
    );
}

#[tokio::test]
async fn attaching_another_tenants_post_is_refused() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let databases = category(&db, "Databases").await;
    let mut db_q = db.clone();
    let mut foreign = Post::filter(Post::fields().id().eq(HELLO))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .unwrap();
    foreign = toasty::create!(Post {
        tenant_id: TenantId::from(SIDE_TENANT),
        title: "Foreign",
        body: "",
        status: foreign.status,
        featured: false,
        created_at: foreign.created_at,
        cover_id: None,
        tags: String::new(),
        seo: foreign.seo,
        publication: foreign.publication,
        author_id: foreign.author_id,
    })
    .exec(&mut db_q)
    .await
    .unwrap();
    let resp = client
        .submit(
            &format!("/admin/categories/{databases}/-/relations/posts/-/actions/attach"),
            &form_body(&[("record", &foreign.id.to_string())]),
        )
        .await;
    assert_eq!(resp.status(), 200, "the dialog's input comes back refused");
    let html = body_string(resp).await;
    assert_eq!(
        field_error(&html, "record").as_deref(),
        Some("Post is invalid")
    );
    assert!(filed(&db, foreign.id).await.is_empty());
}

#[tokio::test]
async fn detaching_unfiles_a_row_or_the_selection() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let (engineering, databases) = (
        category(&db, "Engineering").await,
        category(&db, "Databases").await,
    );
    let relation = format!("/admin/categories/{databases}/-/relations/posts");
    let resp = client
        .submit(&format!("{relation}/{HELLO}/-/actions/detach"), "")
        .await;
    assert_eq!(resp.status(), 303);
    assert_eq!(filed(&db, HELLO).await, vec![engineering], "only the row");
    assert_eq!(filed(&db, PLANS).await, vec![databases]);

    let resp = client
        .submit(
            &format!("{relation}/-/actions/detach"),
            &form_body(&[("ids", &PLANS.to_string())]),
        )
        .await;
    assert_eq!(resp.status(), 303);
    assert!(filed(&db, PLANS).await.is_empty());
}

#[tokio::test]
async fn detaching_a_post_the_category_does_not_hold_404s() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let databases = category(&db, "Databases").await;
    let resp = client
        .submit(
            &format!("/admin/categories/{databases}/-/relations/posts/-/actions/detach"),
            &form_body(&[("ids", &format!("{PLANS},{SECOND}"))]),
        )
        .await;
    assert_eq!(resp.status(), 404);
    assert_eq!(
        filed(&db, PLANS).await,
        vec![databases],
        "a refused selection unfiles none of it"
    );
}

/// The demo tenant's categories, which the post form offers.
#[tokio::test]
async fn the_post_form_offers_the_tenants_categories() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let side = toasty::create!(Category {
        tenant_id: TenantId::from(SIDE_TENANT),
        name: "Foreign",
    })
    .exec(&mut db_q)
    .await
    .unwrap();
    let html = body_string(client.get("/admin/posts/create").await).await;
    let demo: Vec<Category> = Category::filter(
        Category::fields()
            .tenant_id()
            .eq(TenantId::from(DEMO_TENANT)),
    )
    .exec(&mut db_q)
    .await
    .unwrap();
    for category in demo {
        assert_eq!(
            checked(&html, category.id),
            Some(false),
            "{}",
            category.name
        );
    }
    assert_eq!(checked(&html, side.id), None, "another tenant's category");
}
