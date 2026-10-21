//! Reads a CSV import page: the alert refusing the whole file names why in
//! `data-import-refusal`, each refused value's row carries its row number in
//! `data-import-error` and its form key in `data-import-key`, and each column the import takes
//! carries its key in `data-import-column`.

use tablo_core::dom::import;

use super::dom::{attr_value, tags_where};

/// Why an import refused a whole file, as its alert's `data-import-refusal` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImportRefusal {
    /// The submission carries no file.
    NoFile,
    /// The file is not UTF-8 text.
    Encoding,
    /// A quoted cell is never closed.
    Unterminated,
    /// The file holds no header row.
    NoHeader,
    /// The file holds a header but no row.
    NoRows,
    /// The file holds more rows than one import creates.
    TooManyRows,
    /// The header names a column no field the request may change takes.
    UnknownColumn,
    /// The header names a single-value column twice.
    RepeatedColumn,
}

/// One value an import refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportError {
    /// The row's number as a spreadsheet shows it, the header being row 1.
    pub row: usize,
    /// The form key refused, `None` for the row as a whole or a field the user may not see.
    pub key: Option<String>,
}

/// Why the import page of `html` refused the whole file, or `None` when it did not.
///
/// # Panics
///
/// When the alert names no refusal this version knows: a renderer that drops
/// `data-import-refusal`'s value would otherwise read as any refusal.
pub fn import_refusal(html: &str) -> Option<ImportRefusal> {
    let (_, attrs, _) = tags_where(html, |_, attrs| {
        attr_value(attrs, "data-import-refusal").is_some()
    })
    .into_iter()
    .next()?;
    Some(match attr_value(&attrs, "data-import-refusal").as_deref() {
        Some(import::NO_FILE) => ImportRefusal::NoFile,
        Some(import::ENCODING) => ImportRefusal::Encoding,
        Some(import::UNTERMINATED) => ImportRefusal::Unterminated,
        Some(import::NO_HEADER) => ImportRefusal::NoHeader,
        Some(import::NO_ROWS) => ImportRefusal::NoRows,
        Some(import::TOO_MANY_ROWS) => ImportRefusal::TooManyRows,
        Some(import::UNKNOWN_COLUMN) => ImportRefusal::UnknownColumn,
        Some(import::REPEATED_COLUMN) => ImportRefusal::RepeatedColumn,
        other => panic!("the import alert names no known refusal: {other:?}"),
    })
}

/// The values the import page of `html` lists as refused, in its order; empty when it lists
/// none.
///
/// # Panics
///
/// When an entry's row number does not parse.
pub fn import_errors(html: &str) -> Vec<ImportError> {
    tags_where(html, |_, attrs| {
        attr_value(attrs, "data-import-error").is_some()
    })
    .into_iter()
    .map(|(_, attrs, _)| ImportError {
        row: attr_value(&attrs, "data-import-error")
            .and_then(|row| row.parse().ok())
            .expect("an import error names its row"),
        key: attr_value(&attrs, "data-import-key").filter(|key| !key.is_empty()),
    })
    .collect()
}

/// The form keys the import page of `html` lists as the columns a file may name, in order.
pub fn import_columns(html: &str) -> Vec<String> {
    tags_where(html, |_, attrs| {
        attr_value(attrs, "data-import-column").is_some()
    })
    .into_iter()
    .filter_map(|(_, attrs, _)| attr_value(&attrs, "data-import-column"))
    .collect()
}
