//! The panel's global search: the box in the topbar, the results it lists as the user types, and
//! the results page its form opens.

use showcase::models::{Author, BLOCKED_TENANT, Post, User};
use tablo::testing::{SearchGroup, search_results};

use crate::common::{
    TestClient, body_string, demo_client, form_body, full_db, routers::router_for_tests as router,
    tenanted_db, tenantless_client,
};

const RESULTS_SHARD: &str = "/_topcoat/runtime/shards/tablo-search";

/// The groups the results page lists for `term`.
async fn search(client: &TestClient<'_>, term: &str) -> Vec<SearchGroup> {
    let response = client
        .get(&format!("/admin/-/search?{}", form_body(&[("q", term)])))
        .await;
    assert_eq!(response.status(), 200, "the results page answers {term:?}");
    search_results(&body_string(response).await)
}

fn labels(groups: &[SearchGroup]) -> Vec<&str> {
    groups.iter().map(|group| group.label.as_str()).collect()
}

/// The titles `groups` lists under `label`; empty when no group has that label.
fn titles(groups: &[SearchGroup], label: &str) -> Vec<String> {
    groups
        .iter()
        .filter(|group| group.label == label)
        .flat_map(|group| group.results.iter().map(|result| result.title.clone()))
        .collect()
}

/// Each resource's matches sit under its plural label, the groups in the sidebar's order, and a
/// result opens its record's detail page with the details its resource declares.
#[tokio::test]
async fn a_term_lists_every_resources_matches_in_the_sidebars_order() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let ada = User::filter(User::fields().email().eq("ada@example.com".to_string()))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the seed creates Ada");

    let groups = search(&client, "ada").await;

    assert_eq!(labels(&groups)[..2], ["Users", "Writers"]);
    assert_eq!(titles(&groups, "Users"), ["Ada Lovelace"]);
    assert_eq!(titles(&groups, "Writers"), ["Ada Author"]);
    let result = &groups[0].results[0];
    assert_eq!(result.href, format!("/admin/users/{}", ada.id));
    assert_eq!(
        result.details[0],
        ("Email".to_string(), "ada@example.com".to_string())
    );
    assert_eq!(result.details.len(), 2, "the role follows the email");
}

#[tokio::test]
async fn a_record_matches_when_every_word_of_the_term_does() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let groups = search(&client, "author  ada").await;
    assert_eq!(labels(&groups), ["Writers"]);
    assert_eq!(titles(&groups, "Writers"), ["Ada Author"]);

    assert!(search(&client, "ada turing").await.is_empty());
    assert!(search(&client, "   ").await.is_empty());
}

#[tokio::test]
async fn a_search_lists_only_the_tenants_records() {
    let (db, _t1, t2) = tenanted_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await.tenant(t2);

    let groups = search(&client, "post").await;
    assert_eq!(titles(&groups, "Blog Posts"), ["T2 Post"]);
    assert!(
        search(&client, "t1").await.is_empty(),
        "the other tenant's author, post and comment stay out"
    );
}

/// A request without a tenant lists the resources outside tenancy, and none of the scoped ones.
#[tokio::test]
async fn a_request_without_a_tenant_searches_only_unscoped_resources() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = tenantless_client(&router, &db).await;

    let groups = search(&client, "ada").await;
    assert_eq!(titles(&groups, "Users"), ["Ada Lovelace"]);
    assert!(titles(&groups, "Writers").is_empty());
    assert!(titles(&search(&client, "post").await, "Blog Posts").is_empty());
}

#[tokio::test]
async fn a_trashed_record_leaves_the_results() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    assert_eq!(
        titles(&search(&client, "second post").await, "Blog Posts"),
        ["Second Post"]
    );
    let post = Post::filter(Post::fields().title().eq("Second Post".to_string()))
        .first()
        .exec(&mut db.clone())
        .await
        .unwrap()
        .expect("the seed creates the second post");

    let deleted = client
        .submit(&format!("/admin/posts/{}/delete", post.id), "confirm=1")
        .await;
    assert_eq!(deleted.status(), 303);

    assert!(titles(&search(&client, "second post").await, "Blog Posts").is_empty());
}

/// The blocked tenant's policy refuses the blog resources `ViewAny`: the search lists none of
/// their records, the tenant's own included, and still lists the users.
#[tokio::test]
async fn a_resource_the_policy_refuses_to_list_stays_out() {
    let db = full_db().await;
    toasty::create!(Author {
        tenant_id: BLOCKED_TENANT,
        name: "Ada Blocked",
        email: "ada.blocked@example.com",
    })
    .exec(&mut db.clone())
    .await
    .expect("create an author under the blocked tenant");
    let router = router(db.clone());
    let client = demo_client(&router, &db).await.tenant(BLOCKED_TENANT);

    assert_eq!(labels(&search(&client, "ada").await), ["Users"]);
}

/// A comment the policy refuses to show matches the term and stays out; a visible one shows.
#[tokio::test]
async fn a_record_the_request_may_not_open_stays_out() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    assert_eq!(
        titles(&search(&client, "staging").await, "Comments").len(),
        1
    );
    assert!(titles(&search(&client, "removed").await, "Comments").is_empty());
}

/// The box's results are a shard over the term: a request lists the same results as the page,
/// and a prefix other than its panel's lists none.
#[tokio::test]
async fn the_box_lists_the_results_of_its_term() {
    let db = full_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let shard = |prefix: &str| format!(r#"{{"args":["{prefix}","lovelace"],"signals":{{}}}}"#);

    let response = client.call_procedure(RESULTS_SHARD, &shard("/admin")).await;
    assert_eq!(response.status(), 200);
    let groups = search_results(&body_string(response).await);
    assert_eq!(titles(&groups, "Users"), ["Ada Lovelace"]);

    let response = client.call_procedure(RESULTS_SHARD, &shard("/other")).await;
    assert_eq!(response.status(), 200);
    assert!(search_results(&body_string(response).await).is_empty());
}
