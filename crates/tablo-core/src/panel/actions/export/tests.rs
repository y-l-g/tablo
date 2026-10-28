use super::*;
use crate::{
    Ability, ReadOnly, ResourceDef, lens,
    panel::test_support::{Dummy, dummy_table, seed_dummies},
    test_support::memory_db,
};

/// Drives both chunker walk tests over one table with no filters.
struct ChunkerDummyResource;
impl crate::resource::Resource for ChunkerDummyResource {
    type Model = Dummy;
    type Form = crate::NoForm<Self::Model>;

    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            .slug("dummies")
            .policy_fn(|_cx, ability| matches!(ability, Ability::ViewAny))
            .table(dummy_table())
    }
}

#[test]
fn export_cap_maps_one_row_past_the_limit_to_413() {
    // Maps one row past the limit to 413, exercised at the boundary.
    enforce_export_cap_count(10, 10).unwrap();
    let err = enforce_export_cap_count(11, 10).unwrap_err();
    assert!(
        err.downcast_ref::<topcoat::router::error::ContentTooLargeError>()
            .is_some(),
        "cap must map to content-too-large (413), got {err}"
    );
}

#[tokio::test]
async fn export_chunker_stops_at_a_short_chunk() {
    // Stops at a short chunk; re-fetching cursor-free would rescan.
    let mut db = memory_db(toasty::models!(Dummy)).await;
    for name in ["Ada", "Bob", "Cara"] {
        toasty::create!(Dummy {
            name: name.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = crate::test_support::panel_cx::<ChunkerDummyResource>(&db);
    let table = crate::resource::require_mounted::<ChunkerDummyResource>(&cx)
        .unwrap()
        .table
        .clone();
    let state = crate::table::TableState::default();
    let mut chunker = ExportChunker::new(
        export_base_query(
            &cx,
            &crate::resource::require_mounted::<ChunkerDummyResource>(&cx).unwrap(),
            &table,
            &state,
            &[],
        )
        .expect("tenant scope"),
        crate::Limits::default().export_rows,
    );
    let first = chunker
        .next_chunk(&mut db)
        .await
        .unwrap()
        .expect("short first chunk");
    assert_eq!(first.len(), 3);
    assert!(
        chunker.next_chunk(&mut db).await.unwrap().is_none(),
        "cursor-free chunk must end the walk, not rescan"
    );
    assert!(
        chunker.next_chunk(&mut db).await.unwrap().is_none(),
        "exhausted walk stays exhausted"
    );
}

#[tokio::test]
async fn export_chunker_does_not_rescan_on_exact_multiple_of_chunk() {
    // An exact multiple of the chunk size ends the walk without rescanning.
    let mut db = memory_db(toasty::models!(Dummy)).await;
    seed_dummies(&mut db, EXPORT_CHUNK_ROWS, |i| format!("row-{i:05}")).await;
    let cx = crate::test_support::panel_cx::<ChunkerDummyResource>(&db);
    let table = crate::resource::require_mounted::<ChunkerDummyResource>(&cx)
        .unwrap()
        .table
        .clone();
    let state = crate::table::TableState::default();
    let mut chunker = ExportChunker::new(
        export_base_query(
            &cx,
            &crate::resource::require_mounted::<ChunkerDummyResource>(&cx).unwrap(),
            &table,
            &state,
            &[],
        )
        .expect("tenant scope"),
        crate::Limits::default().export_rows,
    );
    let first = chunker
        .next_chunk(&mut db)
        .await
        .unwrap()
        .expect("full first chunk");
    assert_eq!(first.len(), EXPORT_CHUNK_ROWS);
    assert!(
        chunker.next_chunk(&mut db).await.unwrap().is_none(),
        "exact multiple must end the walk, not rescan from the start"
    );
    assert!(
        chunker.next_chunk(&mut db).await.unwrap().is_none(),
        "exhausted walk stays exhausted"
    );
}

#[tokio::test]
async fn export_and_list_agree_on_rows_and_order() {
    // Drives the same state through list and export and compares rows and order.
    use std::collections::BTreeMap;

    use crate::{
        TextColumn,
        resource::{Resource, ResourceDef},
        table::{SelectFilter, Sort, TableState},
    };

    #[derive(Debug, Clone, toasty::Model, crate::ModelColumns)]
    struct Task {
        #[key]
        #[auto]
        id: uuid::Uuid,
        title: String,
        status: String,
    }
    struct TaskResource;
    impl Resource for TaskResource {
        type Model = Task;
        type Form = crate::NoForm<Self::Model>;

        fn declare() -> ResourceDef<Self> {
            ResourceDef::new().slug("tasks").policy(ReadOnly).table(
                crate::table::Table::new(
                    TextColumn::new(lens!(Task.title)).searchable().sortable(),
                )
                .filters(SelectFilter::new(
                    Task::fields().status(),
                    vec!["published".to_string(), "draft".to_string()],
                )),
            )
        }
    }

    let mut db = memory_db(toasty::models!(Task)).await;
    for (title, status) in [
        ("alpha", "draft"),
        ("bravo", "published"),
        ("charlie", "published"),
        ("delta", "draft"),
        ("echo", "published"),
    ] {
        toasty::create!(Task {
            title: title.to_string(),
            status: status.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = crate::test_support::panel_cx::<TaskResource>(&db);

    // All three declaration steps at once: a search term, a filter and a
    // sort. `?sort=` names the declared sortable column, so both loaders
    // resolve the same ordering without the PK fallback.
    let state = TableState {
        search: Some("a".to_string()),
        filters: BTreeMap::from([("status".to_string(), "published".into())]),
        sort: Some(Sort {
            column: "title".to_string(),
            descending: true,
        }),
        ..TableState::default()
    };

    let table = crate::resource::require_mounted::<TaskResource>(&cx)
        .unwrap()
        .table
        .clone();
    let listed: Vec<String> =
        crate::table::TablePage::load(&cx, &table, TaskResource::query(&cx), &state)
            .await
            .unwrap()
            .rows
            .iter()
            .map(|t| t.title.clone())
            .collect();
    assert_eq!(
        listed,
        ["charlie".to_string(), "bravo".to_string()],
        "the seed must exercise search + filter + sort"
    );

    let mut chunker = ExportChunker::new(
        export_base_query(
            &cx,
            &crate::resource::require_mounted::<TaskResource>(&cx).unwrap(),
            &table,
            &state,
            &[],
        )
        .expect("tenant scope"),
        crate::Limits::default().export_rows,
    );
    let mut exported: Vec<String> = Vec::new();
    while let Some(rows) = chunker.next_chunk(&mut db).await.unwrap() {
        exported.extend(rows.iter().map(|t| t.title.clone()));
    }
    assert_eq!(
        listed, exported,
        "the export must agree with the list on rows and order"
    );
}

#[test]
fn export_filename_cannot_split_the_disposition_header() {
    // Hostile slugs never reach the Content-Disposition header.
    assert_eq!(export_filename("users"), "users.csv");
    for hostile in [
        "a\"b\r\nContent-Length: 0",
        "a\\\"b",
        "\nadmin",
        "bad\u{0}name",
    ] {
        let filename = export_filename(hostile);
        assert!(
            !filename.contains('"')
                && !filename.contains('\\')
                && !filename.contains('\r')
                && !filename.contains('\n')
                && !filename.chars().any(char::is_control),
            "hostile slug {hostile:?} must be defused, got {filename:?}"
        );
        assert!(filename.ends_with(".csv"), "suffix kept: {filename:?}");
    }
    // A slug that defuses to nothing falls back to a usable filename.
    assert_eq!(export_filename(""), "export.csv");
    assert_eq!(export_filename("\""), "export.csv");
    // Bounded header value.
    let long = "x".repeat(500);
    assert_eq!(export_filename(&long).len(), 100 + ".csv".len());
}
