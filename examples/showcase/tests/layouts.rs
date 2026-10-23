//! Layout blocks: the post form's and the post detail page's tabs.

use showcase::models::{Author, Post};
use tablo::testing::{Tab, tabs};

use crate::common::{body_string, demo_client, full_db, routers::router_for_tests as router};

/// The tab of `tabs` whose panel posts `key`.
fn tab_posting<'t>(tabs: &'t [Tab], key: &str) -> &'t Tab {
    tabs.iter()
        .find(|tab| tab.keys.iter().any(|k| k == key))
        .unwrap_or_else(|| panic!("no tab posts `{key}`: {tabs:?}"))
}

/// The create form's body for a post by the first author, followed by `rest`.
async fn create_body(db: &toasty::Db, rest: &str) -> String {
    let mut db = db.clone();
    let author = &Author::all().exec(&mut db).await.unwrap()[0];
    format!("author_id={}&{rest}", author.id)
}

#[tokio::test]
async fn the_create_form_renders_every_tab_and_opens_on_the_first() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let html = body_string(client.get("/admin/posts/-/create").await).await;
    let tabs = tabs(&html);
    assert_eq!(tabs.len(), 4, "{tabs:?}");
    assert_eq!(
        tabs.iter().map(|tab| tab.active).collect::<Vec<_>>(),
        [true, false, false, false]
    );
    // An inactive tab's controls render, so the form posts them.
    assert!(tabs[0].keys.iter().any(|key| key == "author_id"));
    assert!(
        tab_posting(&tabs, "links")
            .keys
            .iter()
            .all(|k| !tabs[0].keys.contains(k))
    );
    assert!(tabs.iter().all(|tab| !tab.errored));
}

#[tokio::test]
async fn a_refused_create_opens_on_the_tab_holding_the_error() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    // The second link has no URL: the refusal sits in the last tab.
    let body = create_body(
        &db,
        "title=Refused&links=0,1&links.0.label=Docs&links.0.url=https%3A%2F%2Fdocs.example\
         &links.1.label=Blog&links.1.url=",
    )
    .await;
    let resp = client.submit("/admin/posts/-/create", &body).await;
    assert!(
        resp.status().is_success(),
        "the refusal re-renders the form"
    );
    let tabs = tabs(&body_string(resp).await);
    let refused = tab_posting(&tabs, "links");
    assert!(refused.active && refused.errored, "{tabs:?}");
    assert_eq!(tabs.iter().filter(|tab| tab.active).count(), 1, "{tabs:?}");
    assert_eq!(tabs.iter().filter(|tab| tab.errored).count(), 1, "{tabs:?}");
}

#[tokio::test]
async fn a_refusal_outside_the_tabs_keeps_the_first_tab_open() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    // The title sits above the tabs.
    let body = create_body(&db, "title=").await;
    let resp = client.submit("/admin/posts/-/create", &body).await;
    let html = body_string(resp).await;
    assert_eq!(
        tablo::testing::field_error(&html, "title"),
        Some(tablo::testing::ErrorKind::Required)
    );
    let tabs = tabs(&html);
    assert!(tabs[0].active, "{tabs:?}");
    assert!(tabs.iter().all(|tab| !tab.errored), "{tabs:?}");
}

#[tokio::test]
async fn the_detail_page_opens_on_its_first_tab() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let id = Post::all().exec(&mut db_q).await.unwrap()[0].id;
    let html = body_string(client.get(&format!("/admin/posts/{id}")).await).await;
    let tabs = tabs(&html);
    assert_eq!(
        tabs.iter().map(|tab| tab.active).collect::<Vec<_>>(),
        [true, false, false],
        "{tabs:?}"
    );
}
