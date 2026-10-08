//! Deleting records, one row or a selection.

use http::header::LOCATION;
use showcase::models::User;
use tablo::NotificationStatus;

use crate::common::{
    body_string, demo_client, response_cookies, routers::router_for_tests as router, row_keys,
    seeded_db, set_cookie_header, user_count,
};

#[tokio::test]
async fn delete_requires_confirmation_and_deletes() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let users = User::all().exec(&mut db_q).await.unwrap();
    let before = users.len();
    let user = users.first().unwrap();
    let id = user.id.to_string();
    let delete_url = format!("/admin/users/{}/delete", id);

    let resp = client.get("/admin/users").await;
    let html = body_string(resp).await;
    let actions =
        tablo::testing::row_actions(&html, &id).expect("the row carries its delete control");
    assert_eq!(
        actions.delete_action.as_deref(),
        Some(delete_url.as_str()),
        "the control must carry the row's POST target"
    );
    // One write form per table: its dialog renders closed, asks before deleting, and posts the
    // confirmation marker.
    assert_eq!(
        html.matches("id=\"table-writes\"").count(),
        1,
        "one write form per table, got {html}"
    );
    let dialog = &html[..html.find("id=\"table-writes\"").unwrap()];
    let dialog = &dialog[dialog.rfind("<dialog").unwrap()..];
    assert!(
        dialog.contains("role=\"alertdialog\"") && !dialog.contains(" open=\"\""),
        "an ordinary list page must render the dialog closed, got {dialog}"
    );
    for needle in [
        "Delete this record?",
        "name=\"confirm\" value=\"1\"",
        "bg-destructive",
    ] {
        assert!(html.contains(needle), "dialog missing {needle} in {html}");
    }

    let resp = client.submit(&delete_url, "").await;
    assert_eq!(
        resp.status(),
        400,
        "an unconfirmed delete POST must refuse, got {}",
        resp.status()
    );

    let resp = client.submit(&delete_url, "confirm=1").await;
    assert!(
        resp.status().is_redirection(),
        "confirmed delete should redirect, got {}",
        resp.status()
    );
    let loc = resp.headers().get(LOCATION).unwrap().to_str().unwrap();
    assert!(
        loc.contains("/admin/users"),
        "redirect to list, got {}",
        loc
    );
    assert_eq!(resp.status(), 303, "a completed delete is a 303 PRG");
    assert!(
        !loc.contains("notification"),
        "the toast must not ride the query, got {loc}"
    );
    assert!(
        set_cookie_header(&resp, "__Host-tablo_notification").is_some(),
        "the flash cookie is set on the redirect"
    );

    assert_eq!(
        user_count(&db).await,
        before - 1,
        "deleting one of {before} must leave {}",
        before - 1
    );
    let mut db_check = db.clone();
    let gone = User::filter(User::fields().id().eq(user.id))
        .first()
        .exec(&mut db_check)
        .await
        .unwrap();
    assert!(gone.is_none(), "deleted user should be gone");

    let resp2 = client.cookies(&response_cookies(&resp)).get(loc).await;
    let html2 = body_string(resp2).await;
    assert!(
        html2.contains("data-sonner-toast"),
        "notification should survive, got {}",
        html2
    );
}

#[tokio::test]
async fn delete_404_for_an_unknown_id() {
    // The wrong-tenant half is `gates::cross_tenant_requests_404_and_touch_nothing`.
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let fake_id = uuid::Uuid::new_v4().to_string();
    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            &format!("/admin/users/{}/delete", fake_id),
            format!("confirm=1&csrf_token={csrf}"),
        )
        .await;
    assert_eq!(
        resp.status(),
        404,
        "unknown id should be 404, got {}",
        resp.status()
    );
}

#[tokio::test]
async fn delete_sso_managed_user_is_forbidden() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let ken = User::filter(User::fields().name().eq("Ken Thompson".to_string()))
        .first()
        .exec(&mut db_q)
        .await
        .unwrap()
        .expect("Ken seed");
    let before = user_count(&db).await;
    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            &format!("/admin/users/{}/delete", ken.id),
            format!("confirm=1&csrf_token={csrf}"),
        )
        .await;
    assert_eq!(
        resp.status(),
        403,
        "protected row delete must be forbidden, got {}",
        resp.status()
    );
    assert_eq!(
        user_count(&db).await,
        before,
        "forbidden delete must remove nothing"
    );
}

#[tokio::test]
async fn bulk_delete_deletes_selected() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let users = User::all().exec(&mut db_q).await.unwrap();
    let before = user_count(&db).await;
    assert!(
        users.len() >= 2,
        "the roster fixture must hold at least two rows to bulk-delete"
    );
    let ids: Vec<String> = users.iter().take(2).map(|u| u.id.to_string()).collect();
    let ids_param = ids.join(",");

    let resp = client.get("/admin/users").await;
    let html = body_string(resp).await;
    assert!(
        html.contains("id=\"table-writes\"")
            && html.contains("formaction=\"/admin/users/bulk-delete\""),
        "list should carry the write form and the bulk delete, got {html}"
    );

    let resp = client
        .submit(
            "/admin/users/bulk-delete",
            &format!("ids={ids_param}&confirm=1"),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "bulk delete should redirect, got {}",
        resp.status()
    );
    let loc = resp.headers().get(LOCATION).unwrap().to_str().unwrap();
    assert!(
        loc.contains("/admin/users"),
        "redirect to list, got {}",
        loc
    );
    assert_eq!(resp.status(), 303, "a completed bulk delete is a 303 PRG");
    assert!(
        !loc.contains("notification"),
        "the toast must not ride the query, got {loc}"
    );
    assert!(
        set_cookie_header(&resp, "__Host-tablo_notification").is_some(),
        "the flash cookie is set on the redirect"
    );

    let remaining = user_count(&db).await;
    assert_eq!(
        remaining,
        before - 2,
        "bulk-deleting 2 of {before} must leave {}",
        before - 2
    );
    let resp2 = client.cookies(&response_cookies(&resp)).get(loc).await;
    let html2 = body_string(resp2).await;
    assert!(
        html2.contains("data-sonner-toast"),
        "notification should survive, got {}",
        html2
    );
}

#[tokio::test]
async fn bulk_delete_without_ids_redirects_with_the_reason() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let before = user_count(&db).await;
    let resp = client
        .submit("/admin/users/bulk-delete", "ids=&confirm=1")
        .await;
    assert_eq!(
        resp.status(),
        303,
        "an empty bulk delete must redirect, not 400, got {}",
        resp.status()
    );
    let loc = resp.headers().get(LOCATION).unwrap().to_str().unwrap();
    assert!(loc.contains("/admin/users"), "redirect to list, got {loc}");
    assert_eq!(
        tablo::testing::notification(&resp).map(|flash| flash.status),
        Some(NotificationStatus::Error),
        "the flash must carry the selection error"
    );
    assert_eq!(
        user_count(&db).await,
        before,
        "an empty bulk delete deletes nothing"
    );
}

#[tokio::test]
async fn bulk_delete_short_fetch_404s_and_deletes_nothing() {
    // A batch naming a missing id comes back short from the
    // tenancy-scoped `IN` fetch and 404s — never half-applied.
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let users = User::all().exec(&mut db_q).await.unwrap();
    let before = users.len();
    let real = users.first().unwrap().id.to_string();
    let missing = uuid::Uuid::new_v4().to_string();
    let resp = client
        .submit(
            "/admin/users/bulk-delete",
            &format!("ids={real},{missing}&confirm=1"),
        )
        .await;
    assert_eq!(
        resp.status(),
        404,
        "short-fetch bulk delete must 404, got {}",
        resp.status()
    );
    assert_eq!(
        user_count(&db).await,
        before,
        "a short-fetch batch must delete nothing"
    );
}

#[tokio::test]
async fn bulk_bar_renders_checkboxes_with_row_keys() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let users = User::all().exec(&mut db_q).await.unwrap();

    let resp = client.get("/admin/users").await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    // The SSO-guarded row renders no checkbox: the offered keys are exactly
    // the roster minus Ken.
    let keys = row_keys(&html);
    let mut expected: Vec<String> = users
        .iter()
        .filter(|u| u.name != "Ken Thompson")
        .map(|u| u.id.to_string())
        .collect();
    expected.sort();
    let mut keys_sorted = keys.clone();
    keys_sorted.sort();
    assert_eq!(
        keys_sorted, expected,
        "the page must offer every allowed row key in {html}"
    );
    let ada = users.iter().find(|u| u.name == "Ada Lovelace").unwrap();
    let resp = client.get("/admin/users?q=Ada").await;
    let html = body_string(resp).await;
    assert_eq!(
        row_keys(&html),
        vec![ada.id.to_string()],
        "only the filtered row should be selectable in {}",
        html
    );
    let ids_param = users
        .iter()
        .take(2)
        .map(|u| u.id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let resp = client
        .submit(
            "/admin/users/bulk-delete",
            &format!("ids={ids_param}&confirm=1"),
        )
        .await;
    assert!(
        resp.status().is_redirection(),
        "checkbox-joined bulk delete should redirect, got {}",
        resp.status()
    );
}

/// Select-all over the seeded roster. Ken renders no checkbox — the
/// SSO guard denies his delete — so the browser's select-all collects the
/// other seven and the handler deletes them, instead of refusing the whole
/// batch over the one row the resource protects.
#[tokio::test]
async fn select_all_skips_the_denied_row_and_deletes_the_rest() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let csrf = uuid::Uuid::new_v4().to_string();
    let mut db_q = db.clone();
    let users = User::all().exec(&mut db_q).await.unwrap();
    let before = users.len();
    let ken = users
        .iter()
        .find(|u| u.name == "Ken Thompson")
        .expect("the seeded SSO-managed row");
    assert!(
        before > 1,
        "the roster needs rows beyond Ken for the batch to prove a deletion"
    );

    let html = body_string(client.get("/admin/users").await).await;
    let denied =
        tablo::testing::row_actions(&html, &ken.id.to_string()).expect("the denied row renders");
    assert_eq!(
        denied.view.as_deref(),
        Some(format!("/admin/users/{}", ken.id).as_str()),
        "the denied row keeps its View link: {html}"
    );
    assert_eq!(denied.edit, None, "the denied row renders no Edit: {html}");
    assert_eq!(
        denied.delete_action, None,
        "the denied row renders no Delete action: {html}"
    );
    assert!(
        !row_keys(&html).contains(&ken.id.to_string()),
        "the denied row must render no checkbox, got {html}"
    );
    let ada = users
        .iter()
        .find(|u| u.name == "Ada Lovelace")
        .expect("the seeded allowed row");
    let allowed = tablo::testing::row_actions(&html, &ada.id.to_string())
        .expect("an allowed row has actions");
    assert_eq!(
        allowed.view.as_deref(),
        Some(format!("/admin/users/{}", ada.id).as_str()),
        "an allowed row keeps its View link, got {html}"
    );
    assert_eq!(
        allowed.edit.as_deref(),
        Some(format!("/admin/users/{}/edit", ada.id).as_str()),
        "an allowed row keeps its Edit link, got {html}"
    );
    assert_eq!(
        allowed.delete_action.as_deref(),
        Some(format!("/admin/users/{}/delete", ada.id).as_str()),
        "an allowed row keeps its Delete action, got {html}"
    );

    let ids = row_keys(&html);
    let mut expected: Vec<String> = users
        .iter()
        .filter(|u| u.id != ken.id)
        .map(|u| u.id.to_string())
        .collect();
    expected.sort();
    let mut ids_sorted = ids.clone();
    ids_sorted.sort();
    assert_eq!(
        ids_sorted, expected,
        "select-all must offer every row but Ken's, got {ids:?}"
    );

    let resp = client
        .csrf(&csrf)
        .post_form(
            "/admin/users/bulk-delete",
            format!("ids={}&confirm=1&csrf_token={csrf}", ids.join(",")),
        )
        .await;
    assert_eq!(
        resp.status(),
        303,
        "select-all over the roster must not 403, got {}",
        resp.status()
    );
    assert_eq!(
        user_count(&db).await,
        1,
        "every allowed row must be deleted, leaving Ken"
    );
    let mut db_check = db.clone();
    let remaining = User::all().exec(&mut db_check).await.unwrap();
    assert_eq!(remaining.len(), 1, "only the denied row survives");
    assert_eq!(remaining[0].name, "Ken Thompson");
}

#[tokio::test]
async fn bulk_delete_without_confirmation_is_refused() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;
    let mut db_q = db.clone();
    let users = User::all().exec(&mut db_q).await.unwrap();
    let before = users.len();
    let id = users[0].id.to_string();

    let resp = client
        .submit("/admin/users/bulk-delete", &format!("ids={id}"))
        .await;
    assert_eq!(
        resp.status(),
        400,
        "an unconfirmed bulk delete must be refused, got {}",
        resp.status()
    );

    assert_eq!(
        user_count(&db).await,
        before,
        "a refused batch deletes nothing"
    );
}
