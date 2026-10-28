//! `Limits` — the bounds a panel holds each request to.

use topcoat::context::Cx;

/// The bounds a panel holds each request to, which
/// [`Panel::limits`](crate::Panel::limits) sets. Each one keeps a request to a known size: a
/// request past one is refused whole, never served in part.
///
/// ```rust
/// # use tablo_core::{Limits, Panel};
/// let panel = Panel::new("admin").limits(
///     Limits::default()
///         .export_rows(50_000)
///         .body_bytes(50 * 1024 * 1024),
/// );
/// # let _ = panel;
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub(crate) export_rows: usize,
    pub(crate) import_rows: usize,
    pub(crate) bulk_records: usize,
    pub(crate) body_bytes: usize,
}

impl Default for Limits {
    /// 10,000 exported rows, 1,000 imported rows, 400 selected records and a 10 MiB body.
    fn default() -> Self {
        Self {
            export_rows: 10_000,
            import_rows: 1_000,
            bulk_records: 400,
            body_bytes: 10 * 1024 * 1024,
        }
    }
}

impl Limits {
    /// The most rows a CSV export holds; a list with more answers 413 rather than a partial file.
    /// Each row the export counts is read twice, once to count and once to stream.
    #[must_use]
    pub fn export_rows(mut self, rows: usize) -> Self {
        self.export_rows = rows.max(1);
        self
    }

    /// The most rows one CSV import creates. Every row writes in the request's one transaction,
    /// so a longer file is refused whole.
    #[must_use]
    pub fn import_rows(mut self, rows: usize) -> Self {
        self.import_rows = rows.max(1);
        self
    }

    /// The most records one bulk action runs on. The selection loads as one `IN` list, which
    /// some databases bound: SQLite takes 32,766 values. A selection of every matching record
    /// past it writes nothing and asks the user to narrow the list.
    #[must_use]
    pub fn bulk_records(mut self, records: usize) -> Self {
        self.bulk_records = records.max(1);
        self
    }

    /// The most bytes a request body under the panel's prefix holds, uploaded files included; a
    /// larger body answers 413. A file field's `max_bytes` must stay below it.
    #[must_use]
    pub fn body_bytes(mut self, bytes: usize) -> Self {
        self.body_bytes = bytes.max(1);
        self
    }
}

/// The limits of the request's panel, which entering it puts on the context; the defaults
/// outside one.
pub(crate) fn of(cx: &Cx) -> Limits {
    topcoat::context::try_context::<Limits>(cx)
        .copied()
        .unwrap_or_default()
}
