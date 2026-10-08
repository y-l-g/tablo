use topcoat::Result;

use super::*;
use crate::{
    Ability, ResourceDef, lens,
    panel::test_support::{Dummy, dummy_table, mount, panel_for},
    test_support::memory_db,
};

#[tokio::test]
async fn delete_and_bulk_delete_require_view() {
    // the edit contract extends to deletes — a record that
    // cannot be viewed cannot be deleted by UUID-guessing the route,
    // even with `Delete` allowed.

    use crate::resource::Resource;

    struct ViewDeniedResource;
    impl Resource for ViewDeniedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| match ability {
                    Ability::View(_record) => false,
                    Ability::DeleteAny => true,
                    Ability::Delete(_) => true,
                    _ => false,
                })
                .table(dummy_table())
        }
    }

    let mut db = memory_db(toasty::models!(Dummy)).await;
    let row = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(db, panel_for::<ViewDeniedResource>()).expect("panel builds");
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
    // Single delete: view-denied is 403 despite `Delete` allowed.
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
async fn delete_and_bulk_delete_require_delete_any() {
    // `DeleteAny` is the whole-resource gate the delete chrome
    // follows, so a POST to a resource that leaves it at its default is
    // refused even when the row predicates allow the record.

    use crate::resource::Resource;

    struct RowOnlyResource;
    impl Resource for RowOnlyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| {
                    matches!(ability, Ability::View(_) | Ability::Delete(_))
                })
                .table(dummy_table())
        }
    }

    let mut db = memory_db(toasty::models!(Dummy)).await;
    let row = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(db.clone(), panel_for::<RowOnlyResource>()).expect("panel builds");
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
async fn delete_resolves_the_primary_key_only() {
    // Handlers resolve the URL id as the typed primary key: another column's
    // value (the name) 404s, for single and bulk.

    use crate::resource::Resource;

    struct KeyedResource;
    impl Resource for KeyedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy(|_cx: &Cx, ability: Ability<'_, Dummy>| {
                    matches!(
                        ability,
                        Ability::View(_) | Ability::DeleteAny | Ability::Delete(_)
                    )
                })
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
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

    let mut db = memory_db(toasty::models!(Dummy)).await;
    let row = toasty::create!(Dummy {
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    let router = mount(db, panel_for::<KeyedResource>()).expect("panel builds");
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
    // Single delete with the name 404s — it is not the primary key.
    let name_single = post(
        "/admin/dummies/Ada/delete".to_string(),
        format!("confirm=1&csrf_token={token}"),
    )
    .await;
    assert_eq!(
        name_single.status(),
        http::StatusCode::NOT_FOUND,
        "a non-key value must not resolve, got {}",
        name_single.status()
    );
    // Single delete with the primary key succeeds.
    let record_single = post(
        format!("/admin/dummies/{}/delete", row.id),
        format!("confirm=1&csrf_token={token}"),
    )
    .await;
    assert!(
        record_single.status().is_redirection(),
        "the primary key must delete, got {}",
        record_single.status()
    );
    // Bulk with the name 404s.
    let name_bulk = post(
        "/admin/dummies/bulk-delete".to_string(),
        format!("ids=Ada&confirm=1&csrf_token={token}"),
    )
    .await;
    assert_eq!(
        name_bulk.status(),
        http::StatusCode::NOT_FOUND,
        "a non-key value must not resolve in bulk, got {}",
        name_bulk.status()
    );
    // Bulk with the primary key succeeds.
    let record_bulk = post(
        "/admin/dummies/bulk-delete".to_string(),
        format!("ids={}&confirm=1&csrf_token={token}", row.id),
    )
    .await;
    assert!(
        record_bulk.status().is_redirection(),
        "the primary key must bulk-delete, got {}",
        record_bulk.status()
    );
}
