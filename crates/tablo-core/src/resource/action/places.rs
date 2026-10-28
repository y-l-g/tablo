//! [`Places`]: where an action's button renders.

/// Where an [`Action`](super::Action)'s button renders: [`Action::PLACES`](super::Action::PLACES).
///
/// ```rust
/// # use tablo_core::Places;
/// // On the detail page and the bulk bar, not on each row or the edit page.
/// const PLACES: Places = Places::DETAIL.with(Places::BULK);
/// assert!(PLACES.contains(Places::BULK));
/// assert!(!PLACES.contains(Places::ROW));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Places(u8);

impl Places {
    /// A button in each table row, the related tables' included.
    pub const ROW: Self = Self(1);
    /// A button in the detail page's header.
    pub const DETAIL: Self = Self(1 << 1);
    /// A button in the edit page's header. It posts a form of its own, so it runs on the record
    /// as stored, and a change typed into the edit form and not saved is lost.
    pub const EDIT: Self = Self(1 << 2);
    /// An entry in the bulk bar, run on the selection.
    pub const BULK: Self = Self(1 << 3);
    /// Every place that shows one record: [`ROW`](Self::ROW), [`DETAIL`](Self::DETAIL) and
    /// [`EDIT`](Self::EDIT).
    pub const RECORD: Self = Self(Self::ROW.0 | Self::DETAIL.0 | Self::EDIT.0);
    /// Every place.
    pub const ALL: Self = Self(Self::RECORD.0 | Self::BULK.0);

    /// These places and `other`'s.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every place of `other` is one of these.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether any place of `other` is one of these.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}
