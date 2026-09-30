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
    resource::{Past, Resource, Table, TableState, row_exists_past},
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
        // The scan reads no column, so it loads none of the columns' relations.
        let mut chunker = ExportChunker::new(export_base_query::<R>(cx, &table, &state)?);
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
        let (tx, body) = http_body_util::Channel::<bytes::Bytes, std::io::Error>::new(8);
        let cx2 = cx.clone();
        tokio::spawn(async move {
            let mut tx = tx;
            // The tenant scope was already resolved for the scan against the
            // same `cx`, table and state, so this cannot fail again — but a
            // stream that cannot build its query aborts instead of sending a
            // truncated CSV (moved the seed behind a `Result`).
            let mut chunker = match export_base_query::<R>(&cx2, &table, &state) {
                Ok(query) => ExportChunker::new(table.include_relations(query)),
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

/// The export's filtered and ordered base query: the resource's tenant-scoped
/// [`scoped_query`](crate::resource::scoped_query) with the table's
/// declaration applied through the one routine the list loader uses, so a
/// gated resource cannot export unscoped and the CSV cannot drift from the
/// list. The streaming pass adds the relations the columns include; the
/// visibility scan reads no column and loads none.
fn export_base_query<R: Resource>(
    cx: &Cx,
    table: &Table<R::Model>,
    state: &TableState,
) -> Result<toasty::stmt::Query<toasty::stmt::List<R::Model>>> {
    Ok(table.apply_declaration(crate::resource::scoped_query::<R>(cx)?, state))
}

/// One cursor-chunked pass over an export base query.
///
/// Yields the raw `MAX_EXPORT_ROWS + 1` window (the bounded over-fetch the
/// cap counts within) a chunk at a time and stops when `next_cursor` is `None`,
/// so callers hold one chunk instead of the window. Each chunk after the
/// first resumes from the previous chunk's `next_cursor`; the walk ends when
/// the cursor is absent, because re-fetching cursor-free would rescan from the
/// start. Toasty's `per_page` is an upper bound, not a guarantee, so a chunk
/// shorter than the requested size still continues when it carries a cursor —
/// and an empty page with a cursor continues too, because Toasty may filter a
/// full SQL page down to zero rows in-memory while preserving the cursor.
/// A full window is probed one row past itself, so
/// [`beyond_window`](Self::beyond_window) distinguishes "the table ended" from
/// "the window did".
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
        loop {
            let remaining = (MAX_EXPORT_ROWS + 1).saturating_sub(self.raw_scanned);
            if remaining == 0 {
                // The window is full: one probe row past it says whether
                // the walk stopped on the table's end or on the window's. A full
                // chunk without a cursor cannot be probed and is treated
                // as the end.
                if let Some(cursor) = self.after.clone() {
                    self.beyond_window =
                        row_exists_past(db, self.query.clone(), Past::After(cursor))
                            .await
                            .map_err(crate::db::unavailable)?;
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
                // An empty page ends the walk only when the cursor is absent:
                // Toasty preserves the cursor while filtering rows in-memory,
                // so an empty page with a cursor has more rows past it.
                self.after = loaded.next_cursor;
                if self.after.is_none() {
                    self.exhausted = true;
                    return Ok(None);
                }
                continue;
            }
            self.raw_scanned += loaded.items.len();
            self.after = loaded.next_cursor;
            let items = loaded.items;
            if self.after.is_none() {
                // Absent cursor: the table is exhausted — re-fetching cursor-free
                // would rescan from the start, so stop here. A short chunk with a
                // cursor still continues: `per_page` is an upper bound.
                self.exhausted = true;
                self.after = None;
            }
            return Ok(Some(items));
        }
    }
}

#[cfg(test)]
mod tests;
