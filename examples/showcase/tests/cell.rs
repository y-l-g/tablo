//! A cell's link and copy button: each row's own value, read off its record.

use showcase::models::Author;
use tablo::testing::rows;

use crate::common::{body_string, demo_client, full_db, routers::router_for_tests as router};

#[tokio::test]
async fn an_author_email_links_to_and_copies_that_authors_address() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get("/admin/authors").await).await;
    let authors = Author::all().exec(&mut db.clone()).await.unwrap();
    let rows = rows(&html);
    assert!(!rows.is_empty(), "the demo tenant lists authors: {html}");
    for row in rows {
        let key = row.select_value.expect("an author row carries its key");
        let author = authors
            .iter()
            .find(|author| author.id.to_string() == key)
            .expect("each row is a stored author");
        assert_eq!(row.links, [format!("mailto:{}", author.email)], "{key}");
        assert_eq!(row.copies, std::slice::from_ref(&author.email), "{key}");
    }
}
