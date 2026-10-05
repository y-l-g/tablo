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
    let href = actions
        .delete_href
        .as_deref()
        .expect("the control keeps its fallback href");
    assert!(
        href.starts_with("/admin/users?") && href.ends_with(&format!("delete={id}")),
        "the control must keep its fallback href, got {href}"
    );
    assert_eq!(
        actions.delete_action.as_deref(),
        Some(delete_url.as_str()),
        "the control must carry the row's POST target"
    );
    let row_delete = tag_with(&html, &format!("delete={id}"));
    let dialog_id = attr_value(row_delete, "data-row-delete-trigger");
    assert_eq!(
        html.matches(&format!("id=\"{dialog_id}\"")).count(),
        1,
        "one row dialog per page, got {html}"
    );
    let dialog = tag_with(&html, &format!("id=\"{dialog_id}\""));
    assert!(
        !dialog.contains("open=\"\""),
        "an ordinary list page must render the row dialog closed, got {dialog}"
    );
    let form = tag_with(&html, "data-row-delete-form");
    assert!(
        !form.contains("action="),
        "the closed dialog takes its action from the row control, got {form}"
    );

    let resp = client.get(&format!("/admin/users?delete={id}")).await;
    assert!(resp.status().is_success());
    let html = body_string(resp).await;
    let dialog = tag_with(&html, &format!("id=\"{dialog_id}\""));
    assert!(
        dialog.contains("open=\"\""),
        "?delete= must render the row dialog open, got {dialog}"
    );
    assert_eq!(
        html.matches(&format!("id=\"{dialog_id}\"")).count(),
        1,
        "one row dialog on the page, got {html}"
    );
    let action = format!("action=\"/admin/users/{id}/delete\"");
    for needle in [
        "role=\"alertdialog\"",
        "Delete this record?",
        "data-dialog-close",
        "data-dialog-open-param=\"open\"",
        "bg-destructive",
        action.as_str(),
        "name=\"confirm\"",
    ] {
        assert!(html.contains(needle), "dialog missing {needle} in {html}");
    }
    let from_title = &html[html
        .find("Delete this record?")
        .expect("the row dialog's title")..];
    let cancel = tag_with(from_title, "data-dialog-close");
    assert!(
        cancel.starts_with("<button"),
        "the row dialog's Cancel must be a button, got {cancel}"
    );

    let resp = client
        .get(&format!("/admin/users?delete={id}&open=false"))
        .await;
    let html = body_string(resp).await;
    let dialog = tag_with(&html, &format!("id=\"{dialog_id}\""));
    assert!(
        !dialog.contains("open=\"\""),
        "?open=false must keep the row dialog closed, got {dialog}"
    );
    assert!(
        !dialog.contains("data-dialog-open-param"),
        "a closed dialog has no dismissal to mirror, got {dialog}"
    );

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
    let flash = set_cookie_header(&resp, "__Host-tablo_notification")
        .expect("the flash cookie is set on the redirect");
    assert!(
        flash.contains("Deleted"),
        "the flash carries the action, got {flash}"
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
        html2.contains("Deleted"),
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
    // would deadlock the edit path's `validate_async` pool discipline loudly
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

#[tokio::test]
async fn delete_forms_opt_in_without_changing_the_post() {
    let db = seeded_db().await;
    let router = router(db.clone());
    let client = demo_client(&router, &db).await;

    let resp = client.get("/admin/users").await;
    let html = body_string(resp).await;

    let row_form = tag_with(&html, "data-row-delete-form");
    assert!(
        row_form.contains("method=\"post\"") && row_form.contains("data-mutation-submit"),
        "the row confirm must stay a POST that opts into the client path, got {row_form}"
    );
    assert!(
        !row_form.contains("action="),
        "the closed row dialog takes its action from the row control, got {row_form}"
    );

    let bulk_form = tag_with(&html, "data-bulk-form");
    assert!(
        bulk_form.contains("method=\"post\"") && bulk_form.contains("data-mutation-submit"),
        "the bulk confirm must stay a POST that opts into the client path, got {bulk_form}"
    );
    assert_eq!(
        attr_value(bulk_form, "action"),
        "/admin/users/bulk-delete",
        "the bulk form posts to the batch route, got {bulk_form}"
    );

    // The confirmation the handlers require rides inside the form either way:
    // the client path is an affordance, never the safeguard.
    assert_eq!(
        html.matches("name=\"confirm\" value=\"1\"").count(),
        2,
        "both confirms must carry the marker the handlers require, got {html}"
    );
}

fn tag_with<'a>(html: &'a str, marker: &str) -> &'a str {
    let at = html
        .find(marker)
        .unwrap_or_else(|| panic!("missing {marker} in {html}"));
    let start = html[..at].rfind('<').expect("the marker's opening tag");
    let end = html[start..].find('>').expect("the tag's end") + start;
    &html[start..=end]
}

fn attr_value<'a>(tag: &'a str, name: &str) -> &'a str {
    let at = tag
        .find(&format!("{name}=\""))
        .unwrap_or_else(|| panic!("missing {name} in {tag}"));
    let start = at + name.len() + 2;
    let end = tag[start..].find('"').expect("the value's end") + start;
    &tag[start..end]
}
