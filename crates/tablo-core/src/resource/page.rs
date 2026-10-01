//! One executed page of a [`Table`]: [`TablePage`] and its loader.
//!
//! The table is the declaration and its pure planning
//! ([`Table::apply_declaration`](super::Table)); this module runs it against
//! the database.

use toasty::stmt::{List, Query};
use toasty_core::stmt::Value;
use topcoat::{Result, context::Cx};

use super::{Cursor, Table, TableState};

/// One executed page of rows for `Table::render`.
///
/// Build it from toasty's `Page` via [`Self::from_toasty_page`], which
/// URL-encodes the engine cursors; a `Vec<M>` converts directly into a page
/// with no neighbors. An absent cursor simply means no Previous/Next link is
/// rendered — the chrome never invents pages.
#[derive(Debug, Clone)]
pub struct TablePage<M> {
    /// The rows of this page.
    pub rows: Vec<M>,
    /// Encoded cursor for the next page (`?after=`), when one exists.
    pub next_cursor: Option<String>,
    /// Encoded cursor for the previous page (`?before=`), when one exists.
    pub prev_cursor: Option<String>,
}

impl<M> From<Vec<M>> for TablePage<M> {
    fn from(rows: Vec<M>) -> Self {
        Self {
            rows,
            next_cursor: None,
            prev_cursor: None,
        }
    }
}

impl<M: toasty::schema::Model> TablePage<M> {
    /// Wrap a toasty cursor-pagination result, encoding its cursors for URLs.
    ///
    /// # Errors
    ///
    /// Errors when a cursor contains a value the URL codec cannot represent
    /// (see `crate::cursor`).
    pub fn from_toasty_page(page: toasty::stmt::Page<M>) -> Result<Self> {
        Ok(Self {
            rows: page.items,
            next_cursor: page
                .next_cursor
                .as_ref()
                .map(crate::cursor::encode)
                .transpose()?,
            prev_cursor: page
                .prev_cursor
                .as_ref()
                .map(crate::cursor::encode)
                .transpose()?,
        })
    }
}

impl<M> TablePage<M>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    /// Load the page of `table` that `state` names: `query` with the table's
    /// search, filters and ordering applied, the relations its columns declare
    /// included, and cursor pagination at its page size.
    ///
    /// `query` is the caller's scope. The panel's list passes the resource's
    /// [`scoped_query`](crate::resource::scoped_query); a page that owns its
    /// table passes its own, with the table from
    /// [`panel::wired_table`](crate::panel::wired_table) when it renders the
    /// resource's action chrome.
    ///
    /// # Errors
    ///
    /// A malformed cursor, a cursor the engine rejects for this
    /// ordering (both take the cursor-stripped retry contract), or a database
    /// failure.
    pub async fn load(
        cx: &Cx,
        table: &Table<M>,
        query: Query<List<M>>,
        state: &TableState,
    ) -> Result<Self> {
        // The declaration becomes predicates and an ordering through the one
        // shared routine — the export loader applies the same one to its own
        // seed. The cursor probes below reuse that filtered and ordered query
        // without the columns' includes: they only ask whether a row exists.
        let base_query = table.apply_declaration(query, state);
        let query = table.include_relations(base_query.clone());
        let mut db = crate::db::db(cx);
        let per_page = table.page_size();
        let mut paginated = toasty::stmt::Paginate::new(query, per_page);
        // Toasty cursor pagination takes exactly one cursor, and the state
        // holds at most one.
        match &state.cursor {
            Some(Cursor::After(token)) => {
                paginated = paginated.after(crate::cursor::decode(token)?);
            }
            Some(Cursor::Before(token)) => {
                paginated = paginated.before(crate::cursor::decode(token)?);
            }
            None => {}
        }
        let loaded = paginated
            .exec(&mut db)
            .await
            .map_err(|error| reject_cursor(error.into(), state))?;
        let mut page = TablePage::from_toasty_page(loaded)?;
        // Cursor-existence probes, one per landing direction:
        // the engine sets `next_cursor`/`prev_cursor` optimistically,
        // so a page sitting exactly at a boundary carries a phantom
        // cursor without validation. Each direction probes only the
        // edge that can lie:
        // - forward/first landing: prev is exact (absent on the first page; otherwise the page we
        //   came from exists), next may be phantom at the end boundary → probe next on full pages.
        //   A short page cannot have a next page.
        // - backward landing: next is exact (the page we came from follows), prev may be phantom
        //   when the fetch lands on the first page → probe prev whenever one is reported.
        //
        // Deliberately NOT a `LIMIT per_page+1` fold: the engine
        // derives `next_cursor` from the last *fetched* row, so
        // trimming the extra row would anchor the next link past it —
        // every `(per_page+1)`th row would vanish from forward walks.
        // The probes keep the main fetch's cursors (which point at
        // displayed rows) as the link anchors.
        //
        // Residual (same as ever): a concurrent delete landing between
        // the main fetch and the click can still void a validated
        // cursor — that degrades to the void-window recovery link
        // never to silently skipped rows.
        if matches!(state.cursor, Some(Cursor::Before(_))) {
            if let Some(cursor) = page.prev_cursor.clone() {
                let past = Past::Before(crate::cursor::decode(&cursor)?);
                if !row_exists_past(&mut db, base_query, past)
                    .await
                    .map_err(crate::error::unavailable)?
                {
                    page.prev_cursor = None;
                }
            }
        } else if page.rows.len() == per_page {
            if let Some(cursor) = page.next_cursor.clone() {
                let past = Past::After(crate::cursor::decode(&cursor)?);
                if !row_exists_past(&mut db, base_query, past)
                    .await
                    .map_err(crate::error::unavailable)?
                {
                    page.next_cursor = None;
                }
            }
        } else {
            // Short page → no next, keep prev as-is (has_previous already correct).
            page.next_cursor = None;
        }
        Ok(page)
    }
}

/// Which side of a cursor a probe looks past.
pub(crate) enum Past {
    /// Rows after the cursor, in the query's order.
    After(Value),
    /// Rows before the cursor.
    Before(Value),
}

/// Whether `query` has a row past the cursor.
///
/// Toasty reports a cursor whenever a page comes back full, so a page that
/// ends exactly at the table's edge carries one with nothing behind it.
/// The list loader validates the cursor its links carry with
/// this, and the export asks it whether rows remain past its row cap.
pub(crate) async fn row_exists_past<M>(
    db: &mut toasty::Db,
    query: Query<List<M>>,
    past: Past,
) -> toasty::Result<bool>
where
    M: toasty::schema::Model + Send + Sync + 'static,
{
    let probe = toasty::stmt::Paginate::new(query, 1);
    let probe = match past {
        Past::After(cursor) => probe.after(cursor),
        Past::Before(cursor) => probe.before(cursor),
    };
    Ok(!probe.exec(db).await?.items.is_empty())
}

/// Attribute a failed paginated fetch to the request's cursor.
///
/// A token cut from a different ordering decodes but the engine refuses the
/// statement (`invalid_statement`: its field count no longer matches the
/// query's `ORDER BY`). No other statement this paginated loader builds carries
/// that error while the request names a cursor. Such a failure is the cursor's,
/// so it takes the cursor-stripped retry contract instead of re-requesting the
/// identical URL forever; every other failure is the database's, and takes the
/// opaque mapping that keeps the driver's text in the log.
fn reject_cursor(error: topcoat::Error, state: &TableState) -> topcoat::Error {
    let cursored = state.cursor.is_some();
    let rejected = error
        .downcast_ref::<toasty::Error>()
        .is_some_and(toasty::Error::is_invalid_statement);
    if cursored && rejected {
        crate::cursor::rejected(&error)
    } else {
        crate::error::unavailable(error)
    }
}
