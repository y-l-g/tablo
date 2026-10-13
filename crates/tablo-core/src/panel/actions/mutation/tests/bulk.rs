use topcoat::Result;

use super::*;
use crate::{
    Ability, ResourceDef, lens,
    panel::test_support::{Dummy, dummy_table, mount, panel_for},
    test_support::memory_db,
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
async fn bulk_delete_caps_ids_and_checks_the_token() {
    use crate::resource::Resource;

    struct KeyedResource;
    impl Resource for KeyedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy_fn(|_cx, ability| {
                    matches!(
                        ability,
                        Ability::View(_) | Ability::DeleteAny | Ability::Delete(_)
                    )
                })
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Dummy.name),
                )))
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
    // The primary key resolves the row.
    let token = uuid::Uuid::new_v4().to_string();
    let ok = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies/-/bulk-delete")
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
        "a bulk delete by primary key succeeds, got {}",
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
                .uri("/admin/dummies/-/bulk-delete")
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
                .uri("/admin/dummies/-/bulk-delete")
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

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("dummies")
                .policy_fn(|_cx, ability| {
                    matches!(
                        ability,
                        Ability::View(_) | Ability::DeleteAny | Ability::Delete(_)
                    )
                })
                .table(dummy_table())
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

    let mut db = memory_db(toasty::models!(Dummy)).await;
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
                .uri("/admin/dummies/-/bulk-delete")
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

/// A selected record whose key holds the wire's separator deletes as one record, and the records
/// its parts would name stay: the row's checkbox writes the key percent-encoded between the
/// token's commas, and the parse decodes it back.
#[tokio::test]
async fn bulk_delete_keeps_a_key_holding_the_separator_whole() {
    use crate::resource::Resource;

    #[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
    struct Tag {
        #[key]
        code: String,
        label: String,
    }

    struct TagResource;
    impl Resource for TagResource {
        type Model = Tag;
        type Form = crate::NoForm<Tag>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new()
                .slug("tags")
                .policy_fn(|_cx, ability| {
                    matches!(
                        ability,
                        Ability::View(_) | Ability::DeleteAny | Ability::Delete(_)
                    )
                })
                .table(crate::table::Table::new(crate::table::TextColumn::new(
                    lens!(Tag.label),
                )))
        }
    }

    let mut db = memory_db(toasty::models!(Tag)).await;
    for code in ["Doe, John", "Doe", "John", "Ada"] {
        toasty::create!(Tag {
            code: code.to_string(),
            label: format!("Label of {code}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = mount(db.clone(), panel_for::<TagResource>()).expect("panel builds");
    let token = uuid::Uuid::new_v4().to_string();
    // The wire two checked rows write, each key one token, in the form encoding a browser posts
    // the hidden `ids` field with.
    let mut body = form_urlencoded::Serializer::new(String::new());
    body.append_pair("ids", ",Doe%2C%20John,,Ada,")
        .append_pair("confirm", "1")
        .append_pair("csrf_token", &token);
    let response = router
        .handle(
            http::Request::builder()
                .uri("/admin/tags/-/bulk-delete")
                .method(http::Method::POST)
                .header(
                    http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .header(
                    http::header::COOKIE,
                    format!("{}={token}", crate::csrf::COOKIE_NAME),
                )
                .body(Body::from(body.finish()))
                .unwrap(),
        )
        .await;
    assert!(
        response.status().is_redirection(),
        "both selected records must delete, got {}",
        response.status()
    );
    let mut handle = db.clone();
    let mut left: Vec<String> = Tag::all()
        .exec(&mut handle)
        .await
        .unwrap()
        .into_iter()
        .map(|tag| tag.code)
        .collect();
    left.sort();
    assert_eq!(left, ["Doe", "John"], "only the two selected keys must go");
}
