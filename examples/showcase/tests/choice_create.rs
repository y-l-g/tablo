//! Creating a post's author from its form: a create framed by the author choice's dialog writes
//! the author in the request's tenant and lands on it, and the dialog leaves the post form whole.

use showcase::{
    app::AuthorResource,
    models::{Author, AuthorFormField, DEMO_TENANT},
};
use tablo::testing::Picked;

use crate::common::{
    body_string, demo_client, full_db,
    routers::{app_for_tests, router_for_tests},
    seeded_db,
};

#[tokio::test]
async fn a_framed_create_writes_the_author_in_the_tenant_and_lands_on_it() {
    let db = seeded_db().await;
    let (router, admin) = app_for_tests(db.clone());
    let client = demo_client(&router, &db).await.panel(&admin);

    let saved = client
        .resource::<AuthorResource>()
        .create()
        .picking()
        .set(AuthorFormField::Name, "Grace Hopper")
        .set(AuthorFormField::Email, "grace@example.com")
        .submit()
        .await;
    let landed = saved.follow().await;

    let grace = Author::filter(Author::fields().name().eq("Grace Hopper"))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the framed create wrote the author");
    assert_eq!(grace.tenant_id, DEMO_TENANT);
    assert_eq!(
        landed.picked(),
        Some(Picked::Option {
            value: grace.id.to_string(),
            label: "Grace Hopper".to_string(),
        })
    );
}

#[tokio::test]
async fn the_author_dialog_leaves_the_fields_after_it_in_the_post_form() {
    let db = full_db().await;
    let router = router_for_tests(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get("/admin/posts/-/create").await).await;
    assert!(
        html.contains("data-create-option-dialog"),
        "the author choice is creatable"
    );
    // A browser ignores a form's start tag inside another form, and ends the outer one at the
    // inner one's end tag.
    let form = &html[html.find("data-form-guard").expect("the post form")..];
    let end = form.find("</form").expect("the post form ends");
    assert!(
        !form[..end].contains("<form"),
        "a form inside the post form"
    );
    assert!(
        form[..end].contains("type=\"submit\""),
        "the post form ends before its save button"
    );
}
