//! The attribute values a `tablo-test` query matches by kind in place of a sentence's wording: why
//! a field's error slot or a table's zero-rows message renders. The renderer that writes a value
//! and the query that reads it name the same constant, so the two cannot drift apart.
//!
//! The hooks a query only reads through, such as `data-column` or a row's `id`, are spelled on
//! each side; a renderer that drops one fails the query that reads it.

/// Why a field's error slot renders, as the slot's `data-error` names it.
pub mod error {
    /// Posted empty, and the field has no blank answer.
    pub const REQUIRED: &str = "required";
    /// Posted a value the field, the resource's validation or the uploader refused.
    pub const INVALID: &str = "invalid";
    /// A choice whose options the user may not read.
    pub const UNAVAILABLE: &str = "unavailable";
}

/// Why a table ignores a requested filter, as its entry in the ignored-filters banner names it in
/// `data-ignored`.
pub mod ignored {
    /// The table declares no filter of that name, or no tab of the `?tab=` key.
    pub const UNKNOWN: &str = "unknown";
    /// The filter selects nothing by the value, or refuses it, as it refuses a key naming a
    /// record the user may not see.
    pub const INVALID: &str = "invalid";
    /// The request carried more filters, values or bytes than the URL codec reads.
    pub const DROPPED: &str = "dropped";
}

/// A table's zero-rows message: what emptied the table, as the message's `data-empty` names it,
/// and the links it offers, as each link's `data-empty-link` names it.
pub mod empty {
    /// The table holds no records.
    pub const NONE: &str = "none";
    /// The request's search matched no record.
    pub const SEARCH: &str = "search";
    /// The request's filters matched no record.
    pub const FILTERS: &str = "filters";
    /// The link that clears the search or the filters.
    pub const CLEAR_LINK: &str = "clear";
    /// The link back to the first page.
    pub const FIRST_PAGE_LINK: &str = "first-page";
}

/// Why a CSV import refuses a whole file, as its alert's `data-import-refusal` names it.
pub mod import {
    /// The submission carries no file.
    pub const NO_FILE: &str = "no-file";
    /// The file is not UTF-8 text.
    pub const ENCODING: &str = "encoding";
    /// A quoted cell is never closed.
    pub const UNTERMINATED: &str = "unterminated";
    /// The file holds no header row.
    pub const NO_HEADER: &str = "no-header";
    /// The file holds a header but no row.
    pub const NO_ROWS: &str = "no-rows";
    /// The file holds more rows than one import creates.
    pub const TOO_MANY_ROWS: &str = "too-many-rows";
    /// A row of the file holds more cells than one import reads.
    pub const TOO_MANY_CELLS: &str = "too-many-cells";
    /// The header names a column no field the request may change takes.
    pub const UNKNOWN_COLUMN: &str = "unknown-column";
    /// The header names a column by a label more than one field carries.
    pub const AMBIGUOUS_COLUMN: &str = "ambiguous-column";
    /// The header names a single-value column twice.
    pub const REPEATED_COLUMN: &str = "repeated-column";
}

/// What a list row's own control does, as its `data-row-action` names it; the row's selection
/// checkbox carries `data-row-select`.
pub mod row {
    /// The link to the record's detail page.
    pub const VIEW: &str = "view";
    /// The link to the record's edit form.
    pub const EDIT: &str = "edit";
    /// The button deleting the record.
    pub const DELETE: &str = "delete";
}
