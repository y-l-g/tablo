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
    /// The table declares no filter of that name.
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
