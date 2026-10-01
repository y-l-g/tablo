use toasty::Db;
use topcoat::Result;

use super::*;
use crate::{
    Ability, Policy,
    panel::test_support::{Dummy, dummy_table, mount, panel_for},
};

#[test]
fn parse_bulk_ids_dedupes_and_trims() {
    assert!(parse_bulk_ids("", MAX_BULK_IDS).is_empty());
    assert_eq!(
        parse_bulk_ids("a, b ,a,, c", MAX_BULK_IDS),
        vec!["a", "b", "c"]
    );
    // The cap bounds the parse too: stop at max + 1 for the handler's 400.
    assert_eq!(parse_bulk_ids("a,b,c,d,e", 3).len(), 4);
}

#[tokio::test]
async fn bulk_delete_caps_ids_and_ignores_display_key() {
    use crate::resource::Resource;

    struct UpperKeyResource;
    impl Resource for UpperKeyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| {
                matches!(
                    ability,
                    Ability::View(_) | Ability::DeleteAny | Ability::Delete(_)
                )
            }
        }
        fn table() -> crate::resource::Table<Dummy> {
            // Non-canonical display key: bulk must still resolve
            // via the typed PK fetch alone. The record key stays canonical
            // The renderer emits it for bulk values, so the
            // display/URL split is exercised, not bypassed.
            crate::resource::Table::new_split(
                |d: &Dummy| d.id.to_string().to_uppercase(),
                |d: &Dummy| d.id.to_string(),
                crate::resource::TextColumn::r#for(Dummy::fields().name(), |d: &Dummy| {
                    d.name.clone()
                }),
            )
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
    let router = mount(db, panel_for::<UpperKeyResource>()).expect("panel builds");
    // Canonical lowercase id succeeds despite an uppercase display key.
    let token = uuid::Uuid::new_v4().to_string();
    let ok = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies/bulk-delete")
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={token}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(format!(
                    "ids={}&confirm=1&csrf_token={token}",
                    row.id
                )))
                .unwrap(),
        )
        .await;
    assert!(
        ok.status().is_redirection(),
        "PK-authenticated bulk must not 404 on display-key mismatch, got {}",
        ok.status()
    );
    // Over-cap batch is a clear 400 before any DB work.
    let big = (0..(MAX_BULK_IDS + 1))
        .map(|i| format!("00000000-0000-0000-0000-{:012}", i))
        .collect::<Vec<_>>()
        .join(",");
    let capped = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies/bulk-delete")
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={token}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(format!(
                    "ids={big}&confirm=1&csrf_token={token}"
                )))
                .unwrap(),
        )
        .await;
    assert_eq!(capped.status(), http::StatusCode::BAD_REQUEST);
    // Missing token is 403.
    let no_token = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies/bulk-delete")
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from(format!("ids={}", row.id)))
                .unwrap(),
        )
        .await;
    assert_eq!(no_token.status(), http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn bulk_delete_mid_loop_failure_deletes_zero_rows() {
    // Fetch, policy checks, and deletes share one
    // framework transaction — an impl that fails halfway rolls everything
    // back instead of half-applying.

    use crate::resource::Resource;

    struct FlakyBulkResource;
    impl Resource for FlakyBulkResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| {
                matches!(
                    ability,
                    Ability::View(_) | Ability::DeleteAny | Ability::Delete(_)
                )
            }
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
        }
        async fn bulk_delete_records(
            _cx: &Cx,
            records: &[Dummy],
            ex: &mut dyn toasty::Executor,
        ) -> Result<()> {
            // Delete the first row, then blow up: without the
            // framework tx the first delete would stick.
            let first = &records[0];
            Dummy::filter(Dummy::fields().id().eq(first.id))
                .delete()
                .exec(&mut *ex)
                .await
                .map_err(topcoat::Error::from)?;
            Err(std::io::Error::other("boom").into())
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for name in ["one", "two"] {
        toasty::create!(Dummy {
            name: name.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let mut db_ids = db.clone();
    let rows = Dummy::all().exec(&mut db_ids).await.unwrap();
    assert_eq!(rows.len(), 2);
    let ids = rows
        .iter()
        .map(|r| r.id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let router = mount(db.clone(), panel_for::<FlakyBulkResource>()).expect("panel builds");
    let token = uuid::Uuid::new_v4().to_string();
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies/bulk-delete")
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={token}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(format!(
                    "ids={ids}&confirm=1&csrf_token={token}"
                )))
                .unwrap(),
        )
        .await;
    assert!(
        resp.status().is_server_error(),
        "mid-loop failure must error, got {}",
        resp.status()
    );
    let rows = Dummy::all().exec(&mut db_ids).await.unwrap();
    assert_eq!(
        rows.len(),
        2,
        "rollback must leave zero rows deleted, got {}",
        2 - rows.len()
    );
}
