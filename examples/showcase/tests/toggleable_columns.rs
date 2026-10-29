//! A list's toggleable columns: the picker, the stored choice, and the relation tables that show
//! the same columns as the list.

use http::header::LOCATION;
use showcase::models::Category;
use tablo::testing::{PickerColumn, column_picker, columns};

use crate::common::{
    body_string, demo_client, form_body, full_db, routers::router_for_tests as router,
};

#[tokio::test]
async fn a_picked_column_shows_on_the_list_and_its_relation_tables() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let html = body_string(client.get("/admin/posts").await).await;
    assert_eq!(
        columns(&html),
        ["title", "status", "featured", "author", "comments"],
        "a column hidden by default waits to be picked"
    );
    assert_eq!(
        column_picker(&html),
        Some(vec![
            PickerColumn {
                name: "status".to_string(),
                shown: true,
            },
            PickerColumn {
                name: "created_at".to_string(),
                shown: false,
            },
        ]),
        "the picker offers the toggleable columns only, each checked as it shows"
    );

    let resp = client
        .submit(
            "/admin/posts/-/columns",
            &form_body(&[("column", "created_at")]),
        )
        .await;
    assert_eq!(resp.status(), 303);
    assert_eq!(
        resp.headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/admin/posts"),
        "the picker lands back on the list"
    );

    let picked = ["title", "featured", "author", "comments", "created_at"];
    let html = body_string(client.get("/admin/posts").await).await;
    assert_eq!(
        columns(&html),
        picked,
        "the unchecked status hides, the checked date shows"
    );

    let mut db = db.clone();
    let databases = Category::filter(Category::fields().name().eq("Databases".to_string()))
        .first()
        .exec(&mut db)
        .await
        .unwrap()
        .expect("a seeded category")
        .id;
    let html = body_string(
        client
            .get(&format!("/admin/categories/{databases}/posts"))
            .await,
    )
    .await;
    assert_eq!(
        columns(&html),
        picked,
        "a relation table over posts shows what the post list shows"
    );
}
