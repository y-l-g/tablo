//! The write routes over test-local resources: the policy and failure paths the showcase's own
//! resources never take.

use tablo::{Ability, NotificationStatus, lens};
use toasty::Db;

use crate::{
    common::{TestClient, body_string, mount, set_cookie_header},
    framework::common::flash,
};

#[tokio::test]
async fn create_policy_deny() {
    use tablo::{Resource, ResourceDef, Table, TextColumn};

    #[derive(Debug, toasty::Model, Clone)]
    struct DummyUser {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
        email: String,
    }

    struct DenyCreateResource;
    impl Resource for DenyCreateResource {
        type Model = DummyUser;
        type Form = DenyCreateForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy(
                    |_cx: &topcoat::context::Cx, ability: Ability<'_, DummyUser>| match ability {
                        Ability::ViewAny => true,
                        Ability::Create => false,
                        _ => false,
                    },
                )
                .table(Table::new(TextColumn::new(lens!(DummyUser.name))))
        }
    }
    #[derive(tablo::RecordForm)]
    #[form(model = DummyUser)]
    struct DenyCreateForm {
        name: String,
    }
    let db = Db::builder()
        .models(toasty::models!(DummyUser))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router = mount(
        db.clone(),
        tablo::Panel::new("admin")
            .auth(tablo::Auth::disabled())
            .resource::<DenyCreateResource>(),
    )
    .expect("panel builds");
    let client = TestClient::new(&router);

    let slug = "deny-creates";
    let create_url = format!("/admin/{}/create", slug);
    // GET create should be 403
    let resp = client.get(&create_url).await;
    assert_eq!(resp.status(), 403, "GET create should be 403 when denied");

    // POST should also be 403 and not create
    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(&create_url, format!("name=test&csrf_token={csrf}"))
        .await;
    assert_eq!(
        resp.status(),
        403,
        "POST create should be 403 when denied, got {}",
        resp.status()
    );
    // Check DB still empty
    let mut db_check = db.clone();
    let count = DummyUser::all().exec(&mut db_check).await.unwrap().len();
    assert_eq!(count, 0, "should not create when denied");
}

/// A write that fails after validation answers a 500 whose body is
/// Topcoat's plain text, so the failure toast cannot render there. The flash
/// cookie rides the 500 response and the toast appears on the next panel page.
/// `notify_write_failure`'s doc comment describes this delivery.
#[tokio::test]
async fn a_failed_write_toasts_on_the_next_panel_page() {
    use tablo::{Resource, ResourceDef, Table, TextColumn};
    use topcoat::context::Cx;

    #[derive(Debug, toasty::Model, Clone)]
    struct Widget {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }
    struct FailingResource;
    impl Resource for FailingResource {
        type Model = Widget;
        type Form = FailingForm;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("widgets")
                .policy(|_cx: &topcoat::context::Cx, ability: Ability<'_, Widget>| {
                    matches!(ability, Ability::ViewAny | Ability::Create)
                })
                .table(Table::new(TextColumn::new(lens!(Widget.name))).paginate(25))
        }

        async fn create_record(
            _cx: &Cx,
            _form: FailingForm,
            _ex: &mut dyn toasty::Executor,
        ) -> topcoat::Result<Widget> {
            // Validation passed; the write itself did not land.
            Err(std::io::Error::other("the write did not land").into())
        }
    }
    #[derive(tablo::RecordForm)]
    #[form(model = Widget)]
    struct FailingForm {
        name: String,
    }
    let db = Db::builder()
        .models(toasty::models!(Widget))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router = mount(
        db,
        tablo::Panel::new("admin")
            .auth(tablo::Auth::disabled())
            .resource::<FailingResource>(),
    )
    .expect("panel builds");
    let client = TestClient::new(&router);

    let csrf = uuid::Uuid::new_v4().to_string();
    let resp = client
        .csrf(&csrf)
        .post_form(
            "/admin/widgets/create",
            format!("name=Widget&csrf_token={csrf}"),
        )
        .await;
    assert_eq!(
        resp.status(),
        500,
        "a failed write is a 500, got {}",
        resp.status()
    );
    // The flash cookie rides the 500 response...
    let flash = set_cookie_header(&resp, "__Host-tablo_notification")
        .expect("the flash cookie must ride the 500 response");
    let cookie_value = flash
        .split(';')
        .next()
        .and_then(|pair| pair.split_once('='))
        .map(|(_, value)| value.to_string())
        .expect("the Set-Cookie names a value");
    // ...whose body is Topcoat's plain text, so it renders no toast.
    let body = body_string(resp).await;
    assert!(
        !body.contains("data-sonner-toast"),
        "the 500 body is plain text, so it renders no toast: {body}"
    );

    // The next panel page consumes the flash and renders the toast.
    let page = client
        .cookie("__Host-tablo_notification", &cookie_value)
        .get("/admin/widgets")
        .await;
    assert!(
        page.status().is_success(),
        "the list page after the failure must answer, got {}",
        page.status()
    );
    let html = body_string(page).await;
    assert!(
        html.contains("data-type=\"error\""),
        "the next panel page must render the failure toast: {html}"
    );
}

#[tokio::test]
async fn forged_delete_runs_no_record_query() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tablo::{Resource, ResourceDef, Table, TextColumn};

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
        type Form = tablo::NoForm<Self::Model>;

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
        tablo::Panel::new("admin")
            .auth(tablo::Auth::disabled())
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

/// The server-side safety net: a hand-crafted POST naming a row the resource refuses to delete
/// skips that row, as a bulk action skips the rows `can_run` refuses, and a selection of refused
/// rows only deletes nothing.
///
/// `View` allows every row here, so only the partial `Delete` deny can
/// spare `b`: with the default-deny `View` in place, dropping the
/// handler's own `Delete` check would leave this test green.
#[tokio::test]
async fn bulk_delete_hand_crafted_partial_deny_skips_the_refused_row() {
    use tablo::{Resource, ResourceDef, Table, TextColumn};

    #[derive(Debug, toasty::Model, Clone)]
    struct DummyUser {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }

    struct PartialDenyResource;
    impl Resource for PartialDenyResource {
        type Model = DummyUser;
        type Form = tablo::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .policy(
                    |_cx: &topcoat::context::Cx, ability: Ability<'_, DummyUser>| match ability {
                        Ability::ViewAny => true,
                        Ability::View(_rec) => true,
                        Ability::DeleteAny => true,
                        Ability::Delete(rec) => rec.name != "b",
                        _ => false,
                    },
                )
                .table(Table::new(TextColumn::new(lens!(DummyUser.name))))
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(DummyUser))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let a = toasty::create!(DummyUser {
        name: "a".to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();
    let b = toasty::create!(DummyUser {
        name: "b".to_string()
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(
        db.clone(),
        tablo::Panel::new("admin")
            .auth(tablo::Auth::disabled())
            .resource::<PartialDenyResource>(),
    )
    .expect("panel builds");
    let client = TestClient::new(&router);
    let slug = "partial-denies";
    let bulk_delete = |ids: String| {
        let csrf = uuid::Uuid::new_v4().to_string();
        let client = &client;
        async move {
            client
                .csrf(&csrf)
                .post_form(
                    &format!("/admin/{slug}/bulk-delete"),
                    format!("ids={ids}&confirm=1&csrf_token={csrf}"),
                )
                .await
        }
    };
    let mut db_check = db.clone();

    let refused = bulk_delete(b.id.to_string()).await;
    assert_eq!(
        refused.status(),
        303,
        "a refused selection returns to the list"
    );
    assert_eq!(
        flash(&refused).status,
        NotificationStatus::Error,
        "a refused selection is an error"
    );
    let remaining = DummyUser::all().exec(&mut db_check).await.unwrap();
    assert_eq!(remaining.len(), 2, "a refused selection deletes nothing");

    let partial = bulk_delete(format!("{},{}", a.id, b.id)).await;
    assert_eq!(partial.status(), 303, "a partial deny returns to the list");
    let partial = flash(&partial);
    assert!(
        partial.status == NotificationStatus::Success && partial.title.contains("(1 of 2 skipped)"),
        "the notification reports the skipped row: {}",
        partial.title
    );
    let remaining = DummyUser::all().exec(&mut db_check).await.unwrap();
    assert_eq!(remaining.len(), 1, "only the refused row survives");
    assert_eq!(remaining[0].name, "b");
}
