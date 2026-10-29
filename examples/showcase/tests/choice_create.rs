//! Creating a post's author from its form: a create framed by the author choice's dialog writes
//! the author in the request's tenant and lands on it.

use showcase::{
    app::AuthorResource,
    models::{Author, AuthorFormField, DEMO_TENANT},
};
use tablo::testing::Picked;

use crate::common::{demo_client, routers::app_for_tests, seeded_db};

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
