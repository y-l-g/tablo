//! CSV export over the tenant-scoped `export_query`: a bounded visibility scan
//! answers the cap before any byte is sent, then a cursor-chunked walk streams
//! the file.

use topcoat::{
    Result,
    context::Cx,
    router::{Body, RouteFuture, error::forbidden},
};

use super::super::gate::gate;
use crate::{
    db::db,
    resource::{OrderMode, Resource, Table, TableState},
};

/// Max receivable rows an export will deliver: the chunked walk
/// scans at most `MAX_EXPORT_ROWS + 1` raw rows and anything past the cap is
/// a 413, so a 100k-row table stays bounded instead of buffering `Vec<Model>`
/// plus `String` without end. A full raw window with rows left beyond it is a
/// 413 too, even when fewer rows are viewable, so the export never
/// returns a partial file.
const MAX_EXPORT_ROWS: usize = 10_000;

/// Rows per cursor chunk on the export walk: each pass fetches
/// this many models at a time instead of materializing the whole export
/// window, so a 10k-row export holds one chunk plus one CSV fragment.
const EXPORT_CHUNK_ROWS: usize = 500;

/// Reject an export whose visible row count ran past the cap.
/// Extracted from the handler so the 413 mapping is testable at the boundary
/// without materializing 10k rows in a test database.
fn enforce_export_cap_count(count: usize) -> Result<(), topcoat::Error> {
    if count > MAX_EXPORT_ROWS {
        Err(topcoat::router::error::content_too_large().into())
    } else {
        Ok(())
    }
}

/// The longest slug a `Content-Disposition` filename keeps: the
/// header value stays bounded even for an oversized override.
const MAX_EXPORT_FILENAME_LEN: usize = 100;

/// Sanitize the export's `Content-Disposition` filename: `slug`
/// is an overridable free-form `String`, and Topcoat route validation accepts
/// quote and CR/LF segments, so a hostile override would otherwise split the
/// response header. Quote, backslash, and control characters are dropped and
/// the length is capped before the `.csv` suffix. Non-ASCII overrides pass
/// through as obs-text (browsers render them; an RFC 6266 `filename*` is
/// future work).
fn export_filename(slug: &str) -> String {
    let safe: String = slug
        .chars()
        .filter(|c| !c.is_control() && *c != '"' && *c != '\\')
        .take(MAX_EXPORT_FILENAME_LEN)
        .collect();
    if safe.is_empty() {
        "export.csv".to_string()
    } else {
        format!("{safe}.csv")
    }
}

/// `?bom=1` opts into a UTF-8 BOM prefix on the CSV body for Excel.
fn export_wants_bom(cx: &Cx) -> bool {
    let Some(parts) = topcoat::context::try_request_context::<http::request::Parts>(cx) else {
        return false;
    };
    let Some(query) = parts.uri.query() else {
        return false;
    };
    form_urlencoded::parse(query.as_bytes()).any(|(k, v)| k == "bom" && v == "1")
}

/// CSV export — the tenant-scoped `export_query` + `Table` filters/sort,
/// downloads `text/csv`.
///
/// Streams the response as a chunked body: the filtered query is
/// walked in cursor chunks ([`EXPORT_CHUNK_ROWS`] rows at a time) and each
/// chunk's CSV is written incrementally, so a 10k-row export holds one chunk
/// plus one CSV fragment instead of `Vec<Model>` + one joined `String`.
///
/// Two passes keep the pre-body contract. First a bounded visibility scan
/// counts receivable rows inside the `MAX_EXPORT_ROWS + 1` raw window — the
/// 413 reflects what the caller may receive, and a window
/// that fills with rows left beyond it is a 413 too, because those rows may
/// be viewable and dropping them would ship a partial file. Both
/// refusals happen before any byte is sent. The scan renders no cell, so it
/// asks for no relation includes. Then the streaming pass re-walks
/// the same window and emits header + rows, loading the includes the rendered
/// columns declared. A concurrent mutation landing between the passes can only
/// fill the window or push the second past the cap — that aborts the stream
/// loudly instead of truncating silently. The header leads the body even when
/// the window is empty, so an empty table downloads a valid CSV
/// rather than a 0-byte file indistinguishable from a failed download.
/// Formula cells are defused per OWASP in [`Table::csv_row`], and `?bom=1`
/// prepends a UTF-8 BOM for Excel interop.
pub(crate) fn resource_export<R: Resource>(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        gate::<R>(cx)?;
        if !R::can_view_any(cx) {
            return Err(forbidden().into());
        }
        let state = TableState::from_cx(cx);
        let table = R::table(cx);
        // Fail closed on unapplied filters: a typo'd `?filters=`
        // must not silently export the unfiltered table.
        if !table.unapplied_filters(&state).is_empty() {
            return Err(topcoat::router::error::bad_request(format!(
                "invalid filters: {}",
                table
                    .unapplied_filters(&state)
                    .iter()
                    .map(|(pair, reason)| format!("{pair} ({reason})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .into());
        }
        // Visibility scan — count receivable rows inside the
        // raw cap window, so the 413 below fires before any response bytes.
        // The scan reads no column, so it passes an empty include set.
        let mut chunker = ExportChunker::new(export_base_query::<R>(
            cx,
            &table,
            &state,
            &crate::resource::IncludeNeeds::default(),
        )?);
        let mut db_handle = db(cx);
        let mut visible = 0usize;
        while let Some(rows) = chunker.next_chunk(&mut db_handle).await? {
            visible += rows.iter().filter(|r| R::can_view(cx, r)).count();
        }
        // The cap counts viewable rows, but only inside the raw window: rows
        // left past it may be viewable too, so a 200 would be a silent
        // truncation.
        if chunker.beyond_window() {
            return Err(topcoat::router::error::content_too_large().into());
        }
        enforce_export_cap_count(visible)?;
        // Streaming pass: re-walk the window, streaming CSV fragments into a bounded
        // channel the response body reads from (chunked, no Content-Length).
        // The walk moves onto a spawned task with an owned `Cx` clone, so a
        // slow consumer back-pressures the fetch instead of holding the
        // handler — and never an unbounded buffer.
        //
        // The window is walked twice on purpose: the row cap must be answered
        // before the first response byte, and the cap counts rows through
        // `R::can_view`, a Rust predicate no `COUNT(*)` can run. Trimming the
        // streaming pass to the cap instead would ship a partial file for a
        // table the caller may not receive in full; the two walks are
        // the price of a fail-closed 413. This is not a truncating `LIMIT 200`.
        let want_bom = export_wants_bom(cx);
        let needs = table.include_needs();
        let (tx, body) = http_body_util::Channel::<bytes::Bytes, std::io::Error>::new(8);
        let cx2 = cx.clone();
        tokio::spawn(async move {
            let mut tx = tx;
            // The tenant scope was already resolved for the scan against the
            // same `cx`, table and state, so this cannot fail again — but a
            // stream that cannot build its query aborts instead of sending a
            // truncated CSV (moved the seed behind a `Result`).
            let mut chunker = match export_base_query::<R>(&cx2, &table, &state, &needs) {
                Ok(query) => ExportChunker::new(query),
                Err(error) => {
                    tracing::error!(resource = R::slug(), error = %error, "export stream failed");
                    tx.abort(std::io::Error::other("export unavailable"));
                    return;
                }
            };
            let mut db_handle = crate::db::db(&cx2);
            // The header leads the body even when the window has no rows
            // an empty table must download a valid CSV, and a
            // 0-byte body cannot be told from a failed download.
            let mut head = table.csv_header();
            // Opt-in BOM for Excel: `?bom=1` prepends U+FEFF
            // so non-ASCII cells open correctly; default stays
            // BOM-free so existing clients/tests see plain UTF-8.
            if want_bom {
                head.insert(0, '\u{FEFF}');
            }
            if tx.send_data(bytes::Bytes::from(head)).await.is_err() {
                return;
            }
            let mut visible = 0usize;
            loop {
                let rows = match chunker.next_chunk(&mut db_handle).await {
                    Ok(Some(rows)) => rows,
                    Ok(None) => break,
                    Err(error) => {
                        tracing::error!(resource = R::slug(), error = %error, "export stream failed");
                        tx.abort(std::io::Error::other("export unavailable"));
                        return;
                    }
                };
                let mut fragment = String::new();
                for row in &rows {
                    if R::can_view(&cx2, row) {
                        visible += 1;
                        if visible > MAX_EXPORT_ROWS {
                            // TOCTOU overrun: rows changed between the scan
                            // and this pass. Abort loudly — never truncate a
                            // 200 CSV silently.
                            tracing::error!(
                                resource = R::slug(),
                                "export overflowed its cap mid-stream"
                            );
                            tx.abort(std::io::Error::other("export overflowed its cap"));
                            return;
                        }
                        fragment.push_str(&table.csv_row(row));
                    }
                }
                if !fragment.is_empty() && tx.send_data(bytes::Bytes::from(fragment)).await.is_err()
                {
                    return;
                }
            }
            if chunker.beyond_window() {
                // Rows inserted between the passes can fill the window here
                // only, so the scan's answer no longer holds.
                tracing::error!(resource = R::slug(), "export window overflowed mid-stream");
                tx.abort(std::io::Error::other("export overflowed its window"));
            }
        });
        let filename = export_filename(&R::slug());
        let res = http::Response::builder()
            .status(200)
            .header(http::header::CONTENT_TYPE, "text/csv; charset=utf-8")
            .header("x-content-type-options", "nosniff")
            .header(
                http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            )
            .body(Body::new(body))
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(res)
    })
}

/// The export's filtered + ordered base query: the resource's
/// [`export_query`](crate::resource::Resource::export_query) — the soft-delete /
/// row-level seam (ADR-0002) narrowed to the relations `needs` asks for
/// and tenant-scoped by the framework on the way in
/// ([`crate::resource::scoped_query_with`]) — with the table's
/// declaration applied through the one shared routine the list loader uses.
/// The visibility scan passes an empty `needs`; the streaming pass passes
/// [`Table::include_needs`], the relations its columns declared.
///
/// The list and the export differ only in the seed query and the ordering mode:
/// the list loads the tenant-scoped `Resource::query_with` with
/// [`OrderMode::List`], the export the tenant-scoped narrowed `export_query`
/// with [`OrderMode::Export`] — whose PK fallback applies whether or not the
/// table paginates, because the chunked cursor walk needs a deterministic order
/// either way. Both pay the same scope, so a gated resource cannot export
/// unscoped.
fn export_base_query<R: Resource>(
    cx: &Cx,
    table: &Table<R::Model>,
    state: &TableState,
    needs: &crate::resource::IncludeNeeds,
) -> Result<toasty::stmt::Query<toasty::stmt::List<R::Model>>> {
    let seed = crate::resource::apply_tenant_scope::<R>(cx, R::export_query(cx, needs))?;
    Ok(table.apply_declaration(seed, state, OrderMode::Export))
}

/// One cursor-chunked pass over an export base query.
///
/// Yields the raw `MAX_EXPORT_ROWS + 1` window (the bounded over-fetch the
/// cap counts within) a chunk at a time and stops at a short chunk,
/// so callers hold one chunk instead of the window. Each chunk after the
/// first resumes from the previous chunk's `next_cursor`; a chunk shorter
/// than the requested size ends the walk, because chaining an absent cursor
/// or re-fetching cursor-free would rescan from the start. A full window is
/// probed one row past itself, so [`beyond_window`](Self::beyond_window)
/// distinguishes "the table ended" from "the window did".
struct ExportChunker<M> {
    query: toasty::stmt::Query<toasty::stmt::List<M>>,
    after: Option<toasty_core::stmt::Value>,
    raw_scanned: usize,
    exhausted: bool,
    beyond_window: bool,
}

impl<M> ExportChunker<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    fn new(query: toasty::stmt::Query<toasty::stmt::List<M>>) -> Self {
        Self {
            query,
            after: None,
            raw_scanned: 0,
            exhausted: false,
            beyond_window: false,
        }
    }

    /// Whether the window filled with rows left past it, so the raw
    /// walk stopped early rather than at the table's end.
    fn beyond_window(&self) -> bool {
        self.beyond_window
    }

    async fn next_chunk(&mut self, db: &mut toasty::Db) -> Result<Option<Vec<M>>, topcoat::Error> {
        if self.exhausted {
            return Ok(None);
        }
        let remaining = (MAX_EXPORT_ROWS + 1).saturating_sub(self.raw_scanned);
        if remaining == 0 {
            // The window is full: one probe row past it says whether
            // the walk stopped on the table's end or on the window's. A full
            // chunk without a cursor cannot be probed and is treated
            // as the end.
            if let Some(cursor) = self.after.clone() {
                let probe = toasty::stmt::Paginate::new(self.query.clone(), 1)
                    .after(cursor)
                    .exec(db)
                    .await
                    .map_err(crate::db::unavailable)?;
                self.beyond_window = !probe.items.is_empty();
            }
            self.exhausted = true;
            return Ok(None);
        }
        let take = remaining.min(EXPORT_CHUNK_ROWS);
        let mut page = toasty::stmt::Paginate::new(self.query.clone(), take);
        if let Some(cursor) = self.after.take() {
            page = page.after(cursor);
        }
        let loaded = page.exec(db).await.map_err(crate::db::unavailable)?;
        if loaded.items.is_empty() {
            self.exhausted = true;
            return Ok(None);
        }
        let full = loaded.items.len() == take;
        self.raw_scanned += loaded.items.len();
        self.after = loaded.next_cursor;
        let items = loaded.items;
        if !full {
            // Short chunk: the table is exhausted — chaining the (absent)
            // cursor, or re-fetching cursor-free, would rescan from the
            // start, so stop here.
            self.exhausted = true;
            self.after = None;
        }
        Ok(Some(items))
    }
}

#[cfg(test)]
mod tests {
    use toasty::Db;

    use super::*;
    use crate::{
        Panel,
        panel::test_support::{Dummy, dummy_table, panel_for, seed_dummies},
    };

    #[tokio::test]
    async fn export_drops_rows_failing_can_view() {
        use http_body_util::BodyExt;

        use crate::resource::Resource;

        struct RowPolicyResource;
        impl Resource for RowPolicyResource {
            type Model = Dummy;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, record: &Dummy) -> bool {
                record.name != "denied"
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
        for name in ["allowed", "denied"] {
            toasty::create!(Dummy {
                name: name.to_string(),
            })
            .exec(&mut db)
            .await
            .unwrap();
        }
        let router = panel_for::<RowPolicyResource>(db)
            .build()
            .expect("panel builds");
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
            "export must not exceed row visibility (GH #86), got {csv}"
        );
    }

    /// the export's base query is
    /// [`Resource::export_query`](crate::resource::Resource::export_query),
    /// handed the includes the rendered columns declared — not everything
    /// [`Resource::query`](crate::resource::Resource::query) loads.
    ///
    /// Two resources over one model differ only in the declaration: both
    /// render a column that reads `parent`, both implement the same narrowed
    /// `export_query`, and only one column declares `.needs(["parent"])`. The
    /// cell therefore reports which query the export actually ran — the
    /// declared include loads the parent, the silent one does not.
    #[tokio::test]
    async fn export_query_narrows_to_the_declared_column_includes() {
        use http_body_util::BodyExt;
        use toasty::stmt::{Include, List, Query};

        use crate::resource::{IncludeNeeds, Resource};

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

        fn with_parent() -> Query<List<Child>> {
            let inc: Include<Child, Parent> = Child::fields().parent().into();
            Query::<List<Child>>::all().include(inc)
        }

        /// The narrowed base query the export asks for: the parent comes along
        /// only when a rendered column declared it.
        fn narrowed(_cx: &Cx, needs: &IncludeNeeds) -> Query<List<Child>> {
            if needs.wants("parent") {
                with_parent()
            } else {
                Query::<List<Child>>::all()
            }
        }

        struct ExportResource<const DECLARES: bool>;
        impl<const DECLARES: bool> Resource for ExportResource<DECLARES> {
            type Model = Child;
            fn slug() -> String {
                if DECLARES { "declared" } else { "bare" }.to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, _record: &Child) -> bool {
                true
            }
            // The list/detail base query always loads the parent; the export
            // narrows to the declaration.
            fn query(_cx: &Cx) -> Query<List<Child>> {
                with_parent()
            }
            fn export_query(cx: &Cx, needs: &IncludeNeeds) -> Query<List<Child>> {
                narrowed(cx, needs)
            }
            fn table(cx: &Cx) -> crate::resource::Table<Child> {
                let column = crate::resource::TextColumn::computed("Parent", |c: &Child| {
                    if c.parent.is_unloaded() {
                        "(unloaded)".to_string()
                    } else {
                        c.parent.get().name.clone()
                    }
                });
                let column = if DECLARES {
                    column.needs(["parent"])
                } else {
                    column
                };
                crate::resource::Table::r#for(cx)
                    .key(|c: &Child| c.id.to_string())
                    .columns(column)
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

        let router = Panel::new("admin")
            .app_context(db)
            .resource::<ExportResource<true>>()
            .resource::<ExportResource<false>>()
            .auth(crate::Auth::disabled())
            .build()
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
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
                true
            }
            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                dummy_table(cx)
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
        let router = panel_for::<ChunkedResource>(db)
            .build()
            .expect("panel builds");
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
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn table(cx: &Cx) -> crate::resource::Table<Dummy> {
                dummy_table(cx)
            }
        }

        let db = Db::builder()
            .models(toasty::models!(Dummy))
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db.push_schema().await.unwrap();
        let router = panel_for::<EmptyResource>(db)
            .build()
            .expect("panel builds");
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
    async fn export_visibility_scan_asks_for_no_includes() {
        // The counting pass passes no relation includes; only the streaming
        // pass loads the ones the columns declared. `can_view` observes which
        // query loaded the row: the relation is unloaded in the scan and loaded
        // in the stream, so both counters must fire.
        use std::sync::atomic::{AtomicUsize, Ordering};

        use http_body_util::BodyExt;
        use toasty::stmt::{Include, List, Query};

        use crate::resource::{IncludeNeeds, Resource};

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

        fn with_parent() -> Query<List<Child>> {
            let inc: Include<Child, Parent> = Child::fields().parent().into();
            Query::<List<Child>>::all().include(inc)
        }

        struct ScanResource;
        impl Resource for ScanResource {
            type Model = Child;
            fn slug() -> String {
                "children".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, record: &Child) -> bool {
                if record.parent.is_unloaded() {
                    SCAN_UNLOADED.fetch_add(1, Ordering::SeqCst);
                } else {
                    STREAM_LOADED.fetch_add(1, Ordering::SeqCst);
                }
                true
            }
            fn query(_cx: &Cx) -> Query<List<Child>> {
                with_parent()
            }
            fn query_with(_cx: &Cx, needs: &IncludeNeeds) -> Query<List<Child>> {
                if needs.wants("parent") {
                    with_parent()
                } else {
                    Query::<List<Child>>::all()
                }
            }
            fn table(cx: &Cx) -> crate::resource::Table<Child> {
                crate::resource::Table::r#for(cx)
                    .key(|c: &Child| c.id.to_string())
                    .columns(
                        crate::resource::TextColumn::computed("Parent", |c: &Child| {
                            if c.parent.is_unloaded() {
                                "(unloaded)".to_string()
                            } else {
                                c.parent.get().name.clone()
                            }
                        })
                        .needs(["parent"]),
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

        let router = panel_for::<ScanResource>(db).build().expect("panel builds");
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
        // GH #145 (with), preserved under streaming: visibility is
        // counted before the cap inside the raw MAX+1 window, so interleaved
        // denied rows yield a 200 with the visible subset — never a 413, and
        // no count leak.

        use http_body_util::BodyExt;

        use crate::resource::Resource;

        struct MixedResource;
        impl Resource for MixedResource {
            type Model = Dummy;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, record: &Dummy) -> bool {
                !record.name.starts_with("denied-")
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
        seed_dummies(&mut db, MAX_EXPORT_ROWS + 1, |i| {
            if i % 2 == 0 {
                format!("allowed-{i:05}")
            } else {
                format!("denied-{i:05}")
            }
        })
        .await;
        let router = panel_for::<MixedResource>(db)
            .build()
            .expect("panel builds");
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
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, record: &Dummy) -> bool {
                !record.name.starts_with("denied-")
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
        let router = panel_for::<WindowedResource>(db)
            .build()
            .expect("panel builds");
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
        // a short chunk ends the walk — re-fetching cursor-free
        // would rescan from the start and multiply the visible count past
        // the cap.

        use crate::resource::Resource;

        struct TinyResource;
        impl Resource for TinyResource {
            type Model = Dummy;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
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
        let table = TinyResource::table(&cx);
        let state = crate::resource::TableState::default();
        let mut chunker = ExportChunker::new(
            export_base_query::<TinyResource>(&cx, &table, &state, &table.include_needs())
                .expect("tenant scope"),
        );
        let first = chunker
            .next_chunk(&mut db)
            .await
            .unwrap()
            .expect("short first chunk");
        assert_eq!(first.len(), 3);
        assert!(
            chunker.next_chunk(&mut db).await.unwrap().is_none(),
            "short chunk must end the walk, not rescan"
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
        // through `Table::load`, the export through `export_base_query` — and
        // compares the rows and their order.
        use std::collections::HashMap;

        use crate::resource::{OrderMode, Resource, SelectFilter, Sort, TableState, TextColumn};

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
            fn slug() -> String {
                "tasks".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, _record: &Task) -> bool {
                true
            }
            fn table(cx: &Cx) -> crate::resource::Table<Task> {
                crate::resource::Table::r#for(cx)
                    .id(|t: &Task| t.id.to_string())
                    .columns(
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
            filters: HashMap::from([("status".to_string(), "published".to_string())]),
            sort: Some(Sort {
                column: "title".to_string(),
                descending: true,
            }),
            ..TableState::default()
        };

        let table = TaskResource::table(&cx);
        let listed: Vec<String> = table
            .load(&cx, TaskResource::query(&cx), &state)
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
            export_base_query::<TaskResource>(&cx, &table, &state, &table.include_needs())
                .expect("tenant scope"),
        );
        let mut exported: Vec<String> = Vec::new();
        while let Some(rows) = chunker.next_chunk(&mut db).await.unwrap() {
            exported.extend(rows.iter().map(|t| t.title.clone()));
        }
        assert_eq!(
            listed, exported,
            "the export must agree with the list on rows and order"
        );

        // The two modes differ only where they must: an unordered table pins
        // the export to the PK, while the list keeps the query unordered.
        let unsorted = crate::resource::Table::<Task>::r#for(&cx)
            .id(|t: &Task| t.id.to_string())
            .columns(TextColumn::r#for(Task::fields().title(), |t: &Task| {
                t.title.clone()
            }));
        let neutral = TableState::default();
        assert!(
            unsorted.order_bys_for(&neutral, OrderMode::List).is_empty(),
            "an unpaginated list keeps its query unordered"
        );
        assert_eq!(
            unsorted.order_bys_for(&neutral, OrderMode::Export).len(),
            1,
            "the chunked export walk needs a deterministic order"
        );
    }

    #[tokio::test]
    async fn export_413s_above_the_cap_before_streaming() {
        // GH #172 decision 2: the MAX_EXPORT_ROWS cap stays as the backstop
        // above streaming — decided by the pre-body visibility scan, so the
        // 413 carries no partial CSV.

        use crate::resource::Resource;

        struct CappedResource;
        impl Resource for CappedResource {
            type Model = Dummy;
            fn slug() -> String {
                "dummies".to_string()
            }
            fn can_view_any(_cx: &Cx) -> bool {
                true
            }
            fn can_view(_cx: &Cx, _record: &Dummy) -> bool {
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
        seed_dummies(&mut db, MAX_EXPORT_ROWS + 1, |i| format!("user-{i:05}")).await;
        let router = panel_for::<CappedResource>(db)
            .build()
            .expect("panel builds");
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
}
