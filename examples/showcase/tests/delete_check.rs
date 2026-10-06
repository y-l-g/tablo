use http::header::LOCATION;
use showcase::models::User;
use tablo_core::{Ability, lens};
use toasty::Db;

use crate::common::{
    TestClient, body_string, demo_client, mount, response_cookies,
    routers::router_for_tests as router, seeded_db, set_cookie_header, user_count,
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
    let csrf = uuid::Uuid::new_v4().to_string();

    let resp = client.get("/admin/users").await;
    let html = body_string(resp).await;
    let actions = tablo_test::row_actions(&html, &id).expect("the row carries its delete control");
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
    let dialog = &dialog[..dialog.find('>').unwrap()];
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

    let resp = client
        .csrf(&csrf)
        .post_form(&delete_url, format!("csrf_token={csrf}"))
        .await;
    assert_eq!(
        resp.status(),
        400,
        "an unconfirmed delete POST must refuse, got {}",
        resp.status()
    );

    let resp = client
        .csrf(&csrf)
        .post_form(&delete_url, format!("confirm=1&csrf_token={csrf}"))
        .await;
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
    // Core owns the loader unit; this pins the HTTP
    // route for unknown ids. The wrong-tenant half — a valid CSRF pair from
    // another tenant against this tenant's row — is pinned by
    // `gate_matrix_check::cross_tenant_edit_and_delete_404_and_touch_nothing`;
    // the batch and export paths ride the same seam in `tenancy_check.rs`.
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
async fn forged_delete_runs_no_record_query() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tablo_core::{Resource, ResourceDef, Table, TextColumn};

    static QUERIES: AtomicUsize = AtomicUsize::new(0);
    fn counted_query(_cx: &topcoat::context::Cx) -> toasty::stmt::Query<toasty::stmt::List<Dummy>> {
        QUERIES.fetch_add(1, Ordering::SeqCst);
        toasty::stmt::Query::<toasty::stmt::List<Dummy>>::all()
    }

    #[derive(Debug, toasty::Model, Clone)]
    struct Dummy {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }

    struct CountingResource;
    impl Resource for CountingResource {
        type Model = Dummy;
        type Form = tablo_core::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy(|_cx: &topcoat::context::Cx, ability: Ability<'_, Dummy>| {
                    matches!(
                        ability,
                        Ability::ViewAny
                            | Ability::View(_)
                            | Ability::DeleteAny
                            | Ability::Delete(_)
                    )
                })
                .table(Table::new(TextColumn::new(lens!(Dummy.name))))
        }

        fn query(cx: &topcoat::context::Cx) -> toasty::stmt::Query<toasty::stmt::List<Dummy>> {
            counted_query(cx)
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let rec = toasty::create!(Dummy {
        name: "x".to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(
        db.clone(),
        tablo_core::Panel::new("admin")
            .auth(tablo_core::Auth::disabled())
            .resource::<CountingResource>(),
    )
    .expect("panel builds");
    let client = TestClient::new(&router);
    let delete_url = format!("/admin/countings/{}/delete", rec.id);
    let csrf = uuid::Uuid::new_v4().to_string();
    let cookie_mismatch = uuid::Uuid::new_v4().to_string();

    let resp = client
        .csrf(&csrf)
        .post_form(&delete_url, format!("confirm=1&csrf_token={csrf}"))
        .await;
    assert!(resp.status().is_redirection(), "valid delete redirects");
    assert!(
        QUERIES.load(Ordering::SeqCst) > 0,
        "a confirmed delete must load the record (counter wired)"
    );

    // A forged POST answers 403 without a single record query: the CSRF
    // check runs before the record seam is ever consulted (no find_by_key,
    // no existence oracle). The transaction-open half is pinned
    // by the handler ordering (parse/verify/confirm textually precede
    // `db.transaction()`); a regression that reopened a tx before the fetch
    // would deadlock the edit path's option-check pool discipline loudly
    // rather than silently pass.
    QUERIES.store(0, Ordering::SeqCst);
    for body in [
        format!("confirm=1&csrf_token={csrf}"),
        "confirm=1".to_string(),
    ] {
        let resp = client
            .csrf(&cookie_mismatch)
            .post_form(&delete_url, body)
            .await;
        assert_eq!(resp.status(), 403, "forged delete must 403");
        assert_eq!(
            QUERIES.load(Ordering::SeqCst),
            0,
            "a forged delete must not observe a record query"
        );
    }
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
