use toasty::Db;

use super::*;
use crate::{
    Ability, Panel, Policy, ReadOnly,
    panel::test_support::{Dummy, dummy_table, mount, panel_for, seed_dummies},
};

/// The [`Dummy`] resource both chunker walk tests drive: one table, no
/// filters, so the walk exercises raw cursor paging.
struct ChunkerDummyResource;
impl crate::resource::Resource for ChunkerDummyResource {
    type Model = Dummy;
    type Form = crate::NoForm<Self::Model>;
    fn slug() -> String {
        "dummies".to_string()
    }
    fn policy() -> impl Policy<Dummy> {
        |_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny)
    }
    fn table() -> crate::resource::Table<Dummy> {
        dummy_table()
    }
}

#[tokio::test]
async fn export_drops_rows_failing_view() {
    use http_body_util::BodyExt;

    use crate::resource::Resource;

    struct RowPolicyResource;
    impl Resource for RowPolicyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| match ability {
                Ability::ViewAny => true,
                Ability::View(record) => record.name != "denied",
                _ => false,
            }
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for name in ["allowed", "denied"] {
        toasty::create!(Dummy {
            name: name.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = mount(db, panel_for::<RowPolicyResource>()).expect("panel builds");
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let csv = String::from_utf8_lossy(&body);
    assert!(
        csv.contains("allowed"),
        "export must keep viewable rows, got {csv}"
    );
    assert!(
        !csv.contains("denied"),
        "export must not exceed row visibility, got {csv}"
    );
}

/// The export loads the relations the rendered columns include, and nothing
/// else. Two resources over one model differ only in the declaration: both
/// render a column that reads `parent`, and only one column declares
/// `.include(Child::fields().parent())`. The cell therefore reports which
/// query the export ran: the declared include loads the parent, the silent
/// one does not.
#[tokio::test]
async fn export_loads_the_relations_its_columns_include() {
    use http_body_util::BodyExt;

    use crate::resource::Resource;

    #[derive(Debug, toasty::Model, Clone)]
    struct Parent {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }

    #[derive(Debug, toasty::Model, Clone)]
    struct Child {
        #[key]
        #[auto]
        id: uuid::Uuid,
        label: String,
        #[index]
        parent_id: uuid::Uuid,
        #[belongs_to(key = parent_id, references = id)]
        parent: toasty::Deferred<Parent>,
    }

    struct ExportResource<const DECLARES: bool>;
    impl<const DECLARES: bool> Resource for ExportResource<DECLARES> {
        type Model = Child;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            if DECLARES { "declared" } else { "bare" }.to_string()
        }
        fn policy() -> impl Policy<Child> {
            ReadOnly
        }
        fn table() -> crate::resource::Table<Child> {
            let column = crate::resource::TextColumn::computed("Parent", |c: &Child| {
                if c.parent.is_unloaded() {
                    "(unloaded)".to_string()
                } else {
                    c.parent.get().name.clone()
                }
            });
            let column = if DECLARES {
                column.include(Child::fields().parent())
            } else {
                column
            };
            crate::resource::Table::new(|c: &Child| c.id.to_string(), column)
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Parent, Child))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let parent_id = uuid::Uuid::new_v4();
    toasty::create!(Parent {
        id: parent_id,
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Child {
        label: "row".to_string(),
        parent_id,
    })
    .exec(&mut db)
    .await
    .unwrap();

    let router = mount(
        db,
        Panel::new("admin")
            .resource::<ExportResource<true>>()
            .resource::<ExportResource<false>>()
            .auth(crate::Auth::disabled()),
    )
    .expect("panel builds");

    let csv = |body: bytes::Bytes| String::from_utf8_lossy(&body).into_owned();
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/declared/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let declared = csv(resp.into_body().collect().await.unwrap().to_bytes());
    assert!(
        declared.contains("Ada"),
        "a declared include must reach the export query, got {declared}"
    );
    // The discriminating half: the same cell would read `(unloaded)` had
    // the export asked for the un-narrowed `query`.
    assert!(
        !declared.contains("(unloaded)"),
        "a declared include must be loaded, got {declared}"
    );

    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/bare/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let bare = csv(resp.into_body().collect().await.unwrap().to_bytes());
    assert!(
        bare.contains("(unloaded)"),
        "an include no column declared must not be loaded, got {bare}"
    );
}

#[test]
fn export_bom_flag_reads_bom_query_param() {
    use topcoat::context::CxTestBuilder;

    fn cx_for(uri: &str) -> Cx {
        let (parts, ()) = http::Request::builder()
            .uri(uri)
            .body(())
            .unwrap()
            .into_parts();
        CxTestBuilder::new().request_context(parts).build()
    }

    assert!(export_wants_bom(&cx_for("/admin/users/export?bom=1")));
    assert!(!export_wants_bom(&cx_for("/admin/users/export")));
    assert!(!export_wants_bom(&cx_for("/admin/users/export?bom=0")));
    assert!(!export_wants_bom(&cx_for("/admin/users/export?BOM=1")));
}

#[test]
fn export_cap_maps_one_row_past_the_limit_to_413() {
    // the cap branch must produce a content-too-large error, not
    // just a constant that happens to equal 10_000. Exercised at the
    // boundary.
    enforce_export_cap_count(MAX_EXPORT_ROWS).unwrap();
    let err = enforce_export_cap_count(MAX_EXPORT_ROWS + 1).unwrap_err();
    assert!(
        err.downcast_ref::<topcoat::router::error::ContentTooLargeError>()
            .is_some(),
        "cap must map to content-too-large (413), got {err}"
    );
}

#[tokio::test]
async fn export_streams_csv_in_chunks_with_parity() {
    // the streamed body reassembles byte-for-byte to the
    // buffered CSV (header + rows, BOM variant included), arrives without
    // a Content-Length (chunked), and multi-chunk tables cross chunk
    // boundaries without repeating or dropping rows.

    use http_body_util::BodyExt;

    use crate::resource::Resource;

    struct ChunkedResource;
    impl Resource for ChunkedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            ReadOnly
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
        }
    }

    // 2 * chunk + a tail: crosses two chunk boundaries (500/500/203).
    let total = 2 * EXPORT_CHUNK_ROWS + 203;
    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for i in 0..total {
        toasty::create!(Dummy {
            name: format!("user-{i:05}"),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let router = mount(db, panel_for::<ChunkedResource>()).expect("panel builds");
    let get_csv = async |uri: &str| {
        let resp = router
            .handle(
                http::Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert!(
            resp.status().is_success(),
            "export {uri} failed: {}",
            resp.status()
        );
        let content_length = resp.headers().get(http::header::CONTENT_LENGTH).cloned();
        assert!(
            content_length.is_none(),
            "streamed export must not set Content-Length, got {content_length:?}"
        );
        assert_eq!(
            resp.headers()
                .get(http::header::CONTENT_TYPE)
                .map(|v| v.to_str().unwrap_or("")),
            Some("text/csv; charset=utf-8")
        );
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(body.to_vec()).unwrap()
    };
    let csv = get_csv("/admin/dummies/export").await;
    let mut lines = csv.lines();
    assert_eq!(lines.next(), Some("Name"));
    let mut names: Vec<&str> = lines.collect();
    assert_eq!(names.len(), total);
    // The export pins PK order when no sortable column is declared
    // (: cursor chunks need a deterministic order), so compare as
    // a set — chunking must neither drop nor repeat rows.
    names.sort_unstable();
    let mut expected: Vec<String> = (0..total).map(|i| format!("user-{i:05}")).collect();
    expected.sort();
    assert_eq!(
        names,
        expected.iter().map(String::as_str).collect::<Vec<_>>()
    );
    // BOM variant: same rows, FEFF-prefixed.
    let bom = get_csv("/admin/dummies/export?bom=1").await;
    assert!(bom.starts_with('\u{FEFF}'), "BOM must lead, got {bom:?}");
    assert_eq!(&bom['\u{FEFF}'.len_utf8()..], csv);
}

#[tokio::test]
async fn export_of_an_empty_table_emits_the_header() {
    // the header leads the body even when the window has no rows,
    // so an empty table downloads a valid CSV (header, and the BOM when
    // asked for) instead of a 0-byte file a consumer cannot tell from a
    // failed download.

    use http_body_util::BodyExt;

    use crate::resource::Resource;

    struct EmptyResource;
    impl Resource for EmptyResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| matches!(ability, Ability::ViewAny)
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
        }
    }

    let db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let router = mount(db, panel_for::<EmptyResource>()).expect("panel builds");
    let get = async |uri: &str| {
        let resp = router
            .handle(
                http::Request::builder()
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert!(resp.status().is_success(), "export {uri} failed");
        resp.into_body().collect().await.unwrap().to_bytes()
    };

    assert_eq!(
        get("/admin/dummies/export").await.as_ref(),
        b"Name\n",
        "an empty export is the header line, not a 0-byte body"
    );
    assert_eq!(
        get("/admin/dummies/export?bom=1").await.as_ref(),
        "\u{FEFF}Name\n".as_bytes(),
        "the BOM variant leads with U+FEFF even when empty"
    );
}

#[tokio::test]
async fn export_visibility_scan_loads_no_includes() {
    // The counting pass loads no relation; only the streaming pass loads the
    // ones the columns include. `View` observes which
    // query loaded the row: the relation is unloaded in the scan and loaded
    // in the stream, so both counters must fire.
    use std::sync::atomic::{AtomicUsize, Ordering};

    use http_body_util::BodyExt;

    use crate::resource::Resource;

    static SCAN_UNLOADED: AtomicUsize = AtomicUsize::new(0);
    static STREAM_LOADED: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug, toasty::Model, Clone)]
    struct Parent {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
    }

    #[derive(Debug, toasty::Model, Clone)]
    struct Child {
        #[key]
        #[auto]
        id: uuid::Uuid,
        label: String,
        #[index]
        parent_id: uuid::Uuid,
        #[belongs_to(key = parent_id, references = id)]
        parent: toasty::Deferred<Parent>,
    }

    struct ScanResource;
    impl Resource for ScanResource {
        type Model = Child;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "children".to_string()
        }
        fn policy() -> impl Policy<Child> {
            |_cx: &Cx, ability: Ability<'_, Child>| match ability {
                Ability::ViewAny => true,
                Ability::View(record) => {
                    if record.parent.is_unloaded() {
                        SCAN_UNLOADED.fetch_add(1, Ordering::SeqCst);
                    } else {
                        STREAM_LOADED.fetch_add(1, Ordering::SeqCst);
                    }
                    true
                }
                _ => false,
            }
        }
        fn table() -> crate::resource::Table<Child> {
            crate::resource::Table::new(
                |c: &Child| c.id.to_string(),
                crate::resource::TextColumn::computed("Parent", |c: &Child| {
                    if c.parent.is_unloaded() {
                        "(unloaded)".to_string()
                    } else {
                        c.parent.get().name.clone()
                    }
                })
                .include(Child::fields().parent()),
            )
        }
    }

    SCAN_UNLOADED.store(0, Ordering::SeqCst);
    STREAM_LOADED.store(0, Ordering::SeqCst);
    let mut db = Db::builder()
        .models(toasty::models!(Parent, Child))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    let parent_id = uuid::Uuid::new_v4();
    toasty::create!(Parent {
        id: parent_id,
        name: "Ada".to_string(),
    })
    .exec(&mut db)
    .await
    .unwrap();
    toasty::create!(Child {
        label: "row".to_string(),
        parent_id,
    })
    .exec(&mut db)
    .await
    .unwrap();

    let router = mount(db, panel_for::<ScanResource>()).expect("panel builds");
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/children/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let csv = String::from_utf8(
        resp.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(
        csv.contains("Ada"),
        "the stream must render the include: {csv}"
    );
    assert!(
        SCAN_UNLOADED.load(Ordering::SeqCst) > 0,
        "the counting pass must run without the column includes"
    );
    assert!(
        STREAM_LOADED.load(Ordering::SeqCst) > 0,
        "the streaming pass must load the declared include"
    );
}

#[tokio::test]
async fn export_counts_only_viewable_rows_within_the_window() {
    // Preserved under streaming: visibility is
    // counted before the cap inside the raw MAX+1 window, so interleaved
    // denied rows yield a 200 with the visible subset — never a 413, and
    // no count leak.

    use http_body_util::BodyExt;

    use crate::resource::Resource;

    struct MixedResource;
    impl Resource for MixedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| match ability {
                Ability::ViewAny => true,
                Ability::View(record) => !record.name.starts_with("denied-"),
                _ => false,
            }
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    seed_dummies(&mut db, MAX_EXPORT_ROWS + 1, |i| {
        if i % 2 == 0 {
            format!("allowed-{i:05}")
        } else {
            format!("denied-{i:05}")
        }
    })
    .await;
    let router = mount(db, panel_for::<MixedResource>()).expect("panel builds");
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert!(resp.status().is_success());
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let csv = String::from_utf8(body.to_vec()).unwrap();
    let rows: Vec<&str> = csv.lines().skip(1).collect();
    assert_eq!(rows.len(), (MAX_EXPORT_ROWS + 1).div_ceil(2));
    assert!(rows.iter().all(|r| r.starts_with("allowed-")));
    assert!(!csv.contains("denied-"));
}

#[tokio::test]
async fn export_refuses_when_viewable_rows_lie_past_the_window() {
    // the cap counts viewable rows, but only inside the raw
    // window. With rows left past it, a 200 would be a partial CSV — the
    // export must refuse with the same 413 the cap uses.

    use http_body_util::BodyExt;

    use crate::resource::Resource;

    struct WindowedResource;
    impl Resource for WindowedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            |_cx: &Cx, ability: Ability<'_, Dummy>| match ability {
                Ability::ViewAny => true,
                Ability::View(record) => !record.name.starts_with("denied-"),
                _ => false,
            }
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    // 20 rows past the window: the viewable count inside it stays under
    // the cap, so only the presence of rows beyond it can refuse.
    seed_dummies(&mut db, MAX_EXPORT_ROWS + 1 + 20, |i| {
        if i % 2 == 0 {
            format!("allowed-{i:05}")
        } else {
            format!("denied-{i:05}")
        }
    })
    .await;
    let router = mount(db, panel_for::<WindowedResource>()).expect("panel builds");
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    let status = resp.status();
    assert_eq!(
        status,
        http::StatusCode::PAYLOAD_TOO_LARGE,
        "rows past the raw window must 413, got {status}"
    );
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body_text = String::from_utf8_lossy(&body);
    assert!(
        !body_text.contains("allowed-") && !body_text.contains("denied-"),
        "413 must carry no CSV rows, got {body_text:?}"
    );
}

#[tokio::test]
async fn export_chunker_stops_at_a_short_chunk() {
    // A page without a cursor ends the walk — re-fetching cursor-free
    // would rescan from the start and multiply the visible count past
    // the cap.

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    for name in ["Ada", "Bob", "Cara"] {
        toasty::create!(Dummy {
            name: name.to_string(),
        })
        .exec(&mut db)
        .await
        .unwrap();
    }
    let cx = topcoat::context::CxTestBuilder::new()
        .app_context(db.clone())
        .build();
    let table = ChunkerDummyResource::table();
    let state = crate::resource::TableState::default();
    let mut chunker = ExportChunker::new(
        export_base_query::<ChunkerDummyResource>(&cx, &table, &state).expect("tenant scope"),
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
    // A row count that is an exact multiple of the chunk size
    // must not rescan from the start. On SQLite a full page carries a
    // cursor and the empty follow-up ends the walk, so the walk yields
    // one full chunk then stops; the cursor rule keeps this true even
    // for backends that return a full page without a cursor.

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    seed_dummies(&mut db, EXPORT_CHUNK_ROWS, |i| format!("row-{i:05}")).await;
    let cx = topcoat::context::CxTestBuilder::new()
        .app_context(db.clone())
        .build();
    let table = ChunkerDummyResource::table();
    let state = crate::resource::TableState::default();
    let mut chunker = ExportChunker::new(
        export_base_query::<ChunkerDummyResource>(&cx, &table, &state).expect("tenant scope"),
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
    // both loaders apply the table declaration through one shared
    // routine, so a search term, a filter and a sort cannot reach the list
    // and miss the CSV. This drives the same state through both — the list
    // through `TablePage::load`, the export through `export_base_query` — and
    // compares the rows and their order.
    use std::collections::BTreeMap;

    use crate::resource::{Resource, SelectFilter, Sort, TableState, TextColumn};

    #[derive(Debug, Clone, toasty::Model)]
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
        fn slug() -> String {
            "tasks".to_string()
        }
        fn policy() -> impl Policy<Task> {
            ReadOnly
        }
        fn table() -> crate::resource::Table<Task> {
            crate::resource::Table::new(
                |t: &Task| t.id.to_string(),
                TextColumn::r#for(Task::fields().title(), |t: &Task| t.title.clone())
                    .searchable()
                    .sortable(),
            )
            .filters(SelectFilter::r#for(
                Task::fields().status(),
                vec!["published".to_string(), "draft".to_string()],
            ))
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Task))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
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
    let cx = topcoat::context::CxTestBuilder::new()
        .app_context(db.clone())
        .build();

    // All three declaration steps at once: a search term, a filter and a
    // sort. `?sort=` names the declared sortable column, so both loaders
    // resolve the same ordering without the PK fallback.
    let state = TableState {
        search: Some("a".to_string()),
        filters: BTreeMap::from([("status".to_string(), "published".to_string())]),
        sort: Some(Sort {
            column: "title".to_string(),
            descending: true,
        }),
        ..TableState::default()
    };

    let table = TaskResource::table();
    let listed: Vec<String> =
        crate::resource::TablePage::load(&cx, &table, TaskResource::query(&cx), &state)
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
        export_base_query::<TaskResource>(&cx, &table, &state).expect("tenant scope"),
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

#[tokio::test]
async fn export_413s_above_the_cap_before_streaming() {
    // The MAX_EXPORT_ROWS cap stays as the backstop
    // above streaming — decided by the pre-body visibility scan, so the
    // 413 carries no partial CSV.

    use crate::resource::Resource;

    struct CappedResource;
    impl Resource for CappedResource {
        type Model = Dummy;
        type Form = crate::NoForm<Self::Model>;
        fn slug() -> String {
            "dummies".to_string()
        }
        fn policy() -> impl Policy<Dummy> {
            ReadOnly
        }
        fn table() -> crate::resource::Table<Dummy> {
            dummy_table()
        }
    }

    let mut db = Db::builder()
        .models(toasty::models!(Dummy))
        .connect("sqlite::memory:")
        .await
        .unwrap();
    db.push_schema().await.unwrap();
    seed_dummies(&mut db, MAX_EXPORT_ROWS + 1, |i| format!("user-{i:05}")).await;
    let router = mount(db, panel_for::<CappedResource>()).expect("panel builds");
    let resp = router
        .handle(
            http::Request::builder()
                .uri("/admin/dummies/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(
        resp.status(),
        http::StatusCode::PAYLOAD_TOO_LARGE,
        "one row past the cap must 413"
    );
    use http_body_util::BodyExt;
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body_text = String::from_utf8_lossy(&body);
    assert!(
        !body_text.contains("user-"),
        "413 must carry no CSV rows, got {body_text:?}"
    );
}

#[test]
fn export_filename_cannot_split_the_disposition_header() {
    // `slug()` is an overridable free-form String, so quote and
    // control characters must never reach the Content-Disposition header.
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
