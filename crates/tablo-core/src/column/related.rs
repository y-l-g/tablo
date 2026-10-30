//! What a column reads of the related records it loads: the classification a panel checks at
//! mount.

use std::any::TypeId;

/// What a column reads of the related records it loads, which a panel needs to tell whether the
/// column shows a field a mounted resource restricts. A panel refuses a column that is not
/// [`None`](Related::None) over a model a mounted resource restricts a field of, unless the
/// column reads through a resource the panel mounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Related {
    /// No field of a related record and no label of one: a count, or a column over the row's own
    /// fields.
    None,
    /// The related records of the source of this type: their titles, or their fields masked from
    /// a request the source's field access hides them from.
    Source(TypeId),
    /// Related records under no source: an unscoped relation column.
    Unmasked,
    /// The row itself, which a closure may read through any relation the page loads.
    Row,
}
