//! [`Table::summary`]: the counts a list shows under its rows.

use derive_where::derive_where;
use toasty::stmt::{Expr, List, Query};
use topcoat::{Result, context::Cx};

use super::{Table, TableState};
use crate::{Text, lens::Reads};

/// A figure a list shows under its rows: how many records it selects under the reader's tab,
/// search and filters, across every page, or how many of those a predicate selects.
///
/// A summary counts in the database, one count query per summary each time the list renders,
/// so it reads the whole result set, not the page it shows: mind the cost on a
/// [`Pagination::Cursor`](super::Pagination::Cursor) list too large to count. A summary with no
/// predicate reuses the count a [`Pagination::Pages`](super::Pagination::Pages) list takes for
/// its pager, and a list showing no rows counts nothing. Like a [`ListTab`](super::ListTab),
/// it is not access control: a summary whose predicate reads a field
/// [`field_access`](crate::ResourceDef::field_access) hides from the user does not show to them.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, published: bool }
/// use tablo_core::{Summary, Table, TextColumn, lens};
///
/// Table::new(TextColumn::new(lens!(Post.title)))
///     .summary(Summary::count("Posts"))
///     .summary(
///         Summary::count("Published")
///             .query(Post::fields().published().eq(true))
///             .reading(lens!(Post.published).into()),
///     );
/// ```
#[derive_where(Clone)]
pub struct Summary<M> {
    label: Text,
    /// The records the count admits among the list's; `None` admits every one.
    query: Option<Expr<bool>>,
    /// The fields `query` reads, when declared.
    reads: Option<Reads<M>>,
}

impl<M> Summary<M> {
    /// Count the records the list selects, labelled `label`.
    pub fn count(label: impl Into<Text>) -> Self {
        Self {
            label: label.into(),
            query: None,
            reads: None,
        }
    }

    /// Count only the records `predicate` selects among the list's.
    pub fn query(mut self, predicate: Expr<bool>) -> Self {
        self.query = Some(predicate);
        self
    }

    /// Declare the fields the [`query`](Self::query) reads, which a panel needs once any of its
    /// resources restricts a field with [`field_access`](crate::ResourceDef::field_access): the
    /// summary shows only to a user who may view every one.
    pub fn reading(mut self, reads: Reads<M>) -> Self {
        self.reads = Some(reads);
        self
    }

    /// The label, as a declaration error names the summary.
    pub(crate) fn name(&self) -> String {
        self.label.english()
    }

    /// The fields the summary reads off a row, or `None` when its query does not say. A summary
    /// with no query reads nothing.
    pub(crate) fn reads(&self) -> Option<Reads<M>> {
        match &self.query {
            None => Some(Reads::nothing()),
            Some(_) => self.reads.clone(),
        }
    }
}

impl<M> Table<M> {
    /// Show `summary` under the rows, after the declared ones.
    pub fn summary(mut self, summary: Summary<M>) -> Self {
        self.summaries.push(summary);
        self
    }

    /// The declared summaries, in order.
    pub(crate) fn declared_summaries(&self) -> &[Summary<M>] {
        &self.summaries
    }

    /// Each summary's label and how many rows of `query` it counts under `state`'s tab, search
    /// and filters, but the `refused` filters: one count query per summary, in declaration order,
    /// but a summary with no predicate, which takes `total` when the pager counted the same rows.
    ///
    /// # Errors
    ///
    /// The database failing a count.
    pub(crate) async fn summary_counts(
        &self,
        cx: &Cx,
        query: &Query<List<M>>,
        state: &TableState,
        refused: &[String],
        total: Option<u64>,
    ) -> Result<Vec<(Text, u64)>>
    where
        M: toasty::schema::Model,
    {
        let mut counts = Vec::with_capacity(self.summaries.len());
        if self.summaries.is_empty() {
            return Ok(counts);
        }
        let mut db = crate::db::db(cx)?;
        let narrowed = self.narrow(cx, &db, query.clone(), state, refused);
        for summary in &self.summaries {
            let rows = match (&summary.query, total) {
                (None, Some(total)) => {
                    counts.push((summary.label.clone(), total));
                    continue;
                }
                (Some(predicate), _) => narrowed.clone().filter(predicate.clone()),
                (None, None) => narrowed.clone(),
            };
            let count = crate::toasty_compat::count(rows, &mut db)
                .await
                .map_err(crate::error::unavailable)?;
            counts.push((summary.label.clone(), count));
        }
        Ok(counts)
    }
}
