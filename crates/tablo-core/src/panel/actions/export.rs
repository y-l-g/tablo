//! CSV export over the tenant-scoped `query`: a bounded visibility scan
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
    policy::Ability,
    resource::{Mounted, Resource},
    table::{Past, Table, TableState, row_exists_past},
};

/// Bounds the receivable rows an export delivers; a full raw window with rows left beyond it is a
/// 413 so the export never returns a partial file.
const MAX_EXPORT_ROWS: usize = 10_000;

/// Fetches this many models per cursor chunk on the export walk.
const EXPORT_CHUNK_ROWS: usize = 500;

/// Rejects an export whose visible row count ran past the cap.
fn enforce_export_cap_count(count: usize) -> Result<(), topcoat::Error> {
    if count > MAX_EXPORT_ROWS {
        Err(topcoat::router::error::content_too_large().into())
    } else {
        Ok(())
    }
}

/// Bounds the slug a `Content-Disposition` filename keeps.
const MAX_EXPORT_FILENAME_LEN: usize = 100;

/// Sanitizes the export's `Content-Disposition` filename.
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

/// Streams the tenant-scoped query as a CSV download.
pub(crate) fn resource_export<R: Resource>(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can(cx, Ability::ViewAny) {
            return Err(forbidden().into());
        }
        let state = TableState::from_cx(cx);
        let table = resource.table.clone();
        // Fails closed on unapplied filters.
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
        let mut chunker = ExportChunker::new(export_base_query(cx, &resource, &table, &state)?);
        let mut db_handle = db(cx);
        let mut visible = 0usize;
        while let Some(rows) = chunker.next_chunk(&mut db_handle).await? {
            visible += rows
                .iter()
                .filter(|r| resource.can(cx, Ability::View(r)))
                .count();
        }
        if chunker.beyond_window() {
            return Err(topcoat::router::error::content_too_large().into());
        }
        enforce_export_cap_count(visible)?;
        let want_bom = export_wants_bom(cx);
        let (tx, body) = http_body_util::Channel::<bytes::Bytes, std::io::Error>::new(8);
        let cx2 = cx.clone();
        let filename = export_filename(&resource.slug);
        tokio::spawn(async move {
            let mut tx = tx;
            let mut chunker = match export_base_query(&cx2, &resource, &table, &state) {
                Ok(query) => ExportChunker::new(table.include_relations(query)),
                Err(error) => {
                    tracing::error!(resource = resource.slug, error = %error, "export stream failed");
                    tx.abort(std::io::Error::other("export unavailable"));
                    return;
                }
            };
            let mut db_handle = crate::db::db(&cx2);
            let mut head = table.csv_header();
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
                        tracing::error!(resource = resource.slug, error = %error, "export stream failed");
                        tx.abort(std::io::Error::other("export unavailable"));
                        return;
                    }
                };
                let mut fragment = String::new();
                for row in &rows {
                    if resource.can(&cx2, Ability::View(row)) {
                        visible += 1;
                        if visible > MAX_EXPORT_ROWS {
                            // Aborts when rows changed between the passes.
                            tracing::error!(
                                resource = resource.slug,
                                "export overflowed its cap mid-stream"
                            );
                            tx.abort(std::io::Error::other("export overflowed its cap"));
                            return;
                        }
                        fragment.push_str(&table.csv_row(&cx2, row));
                    }
                }
                if !fragment.is_empty() && tx.send_data(bytes::Bytes::from(fragment)).await.is_err()
                {
                    return;
                }
            }
            if chunker.beyond_window() {
                tracing::error!(
                    resource = resource.slug,
                    "export window overflowed mid-stream"
                );
                tx.abort(std::io::Error::other("export overflowed its window"));
            }
        });
        let res = http::Response::builder()
            .status(200)
            .header(http::header::CONTENT_TYPE, "text/csv; charset=utf-8")
            .header("x-content-type-options", "nosniff")
            .header(
                http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename),
            )
            .body(Body::new(body))?;
        Ok(res)
    })
}

/// Builds the export's filtered and ordered base query from the tenant-scoped query.
fn export_base_query<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    table: &Table<R::Model>,
    state: &TableState,
) -> Result<toasty::stmt::Query<toasty::stmt::List<R::Model>>> {
    Ok(table.apply_declaration(resource.scoped_query(cx)?, state))
}

/// Walks an export base query in cursor chunks.
struct ExportChunker<M> {
    query: toasty::stmt::Query<toasty::stmt::List<M>>,
    after: Option<toasty::stmt::Value>,
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

    /// Reports whether rows remain past the window.
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
                if let Some(cursor) = self.after.clone() {
                    self.beyond_window =
                        row_exists_past(db, self.query.clone(), Past::After(cursor))
                            .await
                            .map_err(crate::error::unavailable)?;
                }
                self.exhausted = true;
                return Ok(None);
            }
            let take = remaining.min(EXPORT_CHUNK_ROWS);
            let mut page = toasty::stmt::Paginate::new(self.query.clone(), take);
            if let Some(cursor) = self.after.take() {
                page = page.after(cursor);
            }
            let loaded = page.exec(db).await.map_err(crate::error::unavailable)?;
            if loaded.items.is_empty() {
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
                // Stops here; re-fetching cursor-free would rescan from the start.
                self.exhausted = true;
                self.after = None;
            }
            return Ok(Some(items));
        }
    }
}

#[cfg(test)]
mod tests;
