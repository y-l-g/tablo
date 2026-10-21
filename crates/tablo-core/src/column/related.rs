//! What a column reads of the related records it loads: the classification a panel checks at
//! mount.

/// What a column reads of the related records it loads, which a panel needs to tell whether the
/// column shows a field a mounted resource restricts. A panel refuses a column that is not
/// [`None`](Related::None) over a model a mounted resource restricts a field of, unless the
/// column reads through a resource the panel mounts: its fields masked, or its title.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Related {
    /// No field of a related record and no label of one: a count, or a column over the row's own
    /// fields.
    None,
    /// The fields of the related records of `source`, each masked from a request its field access
    /// hides it from.
    Masked(&'static str),
    /// The titles of the related records of `source`, which its resource keeps clear of a field it
    /// hides.
    Titled(&'static str),
    /// Related records under no source a panel can mask: an unscoped relation column, or a label
    /// or a field of a source that is not a resource.
    Unmasked,
    /// The row itself, which a closure may read through any relation the page loads.
    Row,
}
