use toasty::Db;
use topcoat::Result;

use super::*;
use crate::panel::test_support::{Dummy, dummy_table, panel_for};

#[tokio::test]
async fn delete_and_bulk_delete_require_can_view() {
    // the edit contract extends to deletes — a record that
    // cannot be viewed cannot be deleted by UUID-guessing the route,
    // even with `can_delete == true`.

    use crate::resource::Resource;

    struct ViewDeniedResource;
    impl Resource for ViewDeniedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
            false
        }
        fn can_delete_any(_cx: &Cx) -> bool {
            true
        }
        fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
            dummy_table(cx)
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let row = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = panel_for::<ViewDeniedResource>(db)
        .build()
        .expect("panel builds");
    let token = uuid::Uuid::new_v4().to_string();
    let post = |uri: String, body: String| {
        router.handle(
            http::Request::builder()
                .uri(uri)
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={token}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(body))
                .unwrap(),
        )
    };
    // Single delete: view-denied is 403 despite can_delete == true.
    let single = post(
        format!("/admin/dummies/{}/delete", row.id),
        format!("confirm=1&csrf_token={token}"),
    )
    .await;
    assert_eq!(
        single.status(),
        http::StatusCode::FORBIDDEN,
        "view-denied single delete must 403, got {}",
        single.status()
    );
    // Bulk delete: same rule, per row.
    let bulk = post(
        "/admin/dummies/bulk-delete".to_string(),
        format!("ids={}&confirm=1&csrf_token={token}", row.id),
    )
    .await;
    assert_eq!(
        bulk.status(),
        http::StatusCode::FORBIDDEN,
        "view-denied bulk delete must 403, got {}",
        bulk.status()
    );
}

#[tokio::test]
async fn delete_and_bulk_delete_require_can_delete_any() {
    // `can_delete_any` is the whole-resource gate the delete chrome
    // follows, so a POST to a resource that leaves it at its default is
    // refused even when the row predicates allow the record.

    use crate::resource::Resource;

    struct RowOnlyResource;
    impl Resource for RowOnlyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
            true
        }
        fn can_delete(_cx: &Cx, _record: &Dummy) -> bool {
            true
        }
        fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
            dummy_table(cx)
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let row = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = panel_for::<RowOnlyResource>(db.clone())
        .build()
        .expect("panel builds");
    let token = uuid::Uuid::new_v4().to_string();
    let post = |uri: String, body: String| {
        router.handle(
            http::Request::builder()
                .uri(uri)
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={token}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(body))
                .unwrap(),
        )
    };
    let single = post(
        format!("/admin/dummies/{}/delete", row.id),
        format!("confirm=1&csrf_token={token}"),
    )
    .await;
    assert_eq!(single.status(), http::StatusCode::FORBIDDEN);
    let bulk = post(
        "/admin/dummies/bulk-delete".to_string(),
        format!("ids={}&confirm=1&csrf_token={token}", row.id),
    )
    .await;
    assert_eq!(bulk.status(), http::StatusCode::FORBIDDEN);
    let mut db = db;
    let remaining = Dummy::all().exec(&mut db).await.unwrap();
    assert_eq!(remaining.len(), 1, "a refused delete writes nothing");
}

#[tokio::test]
async fn delete_resolves_record_key_not_display_key() {
    // GH #168 defect 1 round-trip: the display key projects a non-PK value
    // (the name), the record key carries the typed PK. Handlers must 404
    // the display value and accept the record key, for single and bulk.

    use crate::resource::Resource;

    struct NameKeyResource;
    impl Resource for NameKeyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn can_delete_any(_cx: &Cx) -> bool {
            true
        }
        fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
            true
        }
        fn table(_cx: &Cx) -> crate::resource::Table<Dummy> {
            crate::resource::Table::new_split(
                |d: &Dummy| d.name.clone(),
                |d: &Dummy| d.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                    d.name.clone()
                }),
            )
        }
        async fn delete_record(
            _cx: &Cx,
            _record: &Dummy,
            _ex: &mut dyn toasty::Executor,
        ) -> Result<()> {
            Ok(())
        }
        async fn bulk_delete_records(
            _cx: &Cx,
            _records: &[Dummy],
            _ex: &mut dyn toasty::Executor,
        ) -> Result<()> {
            Ok(())
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let row = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = panel_for::<NameKeyResource>(db)
        .build()
        .expect("panel builds");
    let token = uuid::Uuid::new_v4().to_string();
    let post = |uri: String, body: String| {
        router.handle(
            http::Request::builder()
                .uri(uri)
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={token}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(body))
                .unwrap(),
        )
    };
    // Single delete with the display value 404s — it is not a PK.
    let display_single = post(
        "/admin/dummies/Ada/delete".to_string(),
        format!("confirm=1&csrf_token={token}"),
    )
    .await;
    assert_eq!(
        display_single.status(),
        http::StatusCode::NOT_FOUND,
        "display key must not resolve, got {}",
        display_single.status()
    );
    // Single delete with the record key succeeds.
    let record_single = post(
        format!("/admin/dummies/{}/delete", row.id),
        format!("confirm=1&csrf_token={token}"),
    )
    .await;
    assert!(
        record_single.status().is_redirection(),
        "record key must delete, got {}",
        record_single.status()
    );
    // Bulk with the display value 404s.
    let display_bulk = post(
        "/admin/dummies/bulk-delete".to_string(),
        format!("ids=Ada&confirm=1&csrf_token={token}"),
    )
    .await;
    assert_eq!(
        display_bulk.status(),
        http::StatusCode::NOT_FOUND,
        "display key must not resolve in bulk, got {}",
        display_bulk.status()
    );
    // Bulk with the record key succeeds.
    let record_bulk = post(
        "/admin/dummies/bulk-delete".to_string(),
        format!("ids={}&confirm=1&csrf_token={token}", row.id),
    )
    .await;
    assert!(
        record_bulk.status().is_redirection(),
        "record key must bulk-delete, got {}",
        record_bulk.status()
    );
}
