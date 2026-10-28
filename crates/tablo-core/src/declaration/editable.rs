//! The declaration mistakes of a column editing its field in place, and their wording.

use std::fmt;

/// Why a column cannot edit its field in place:
/// [`DeclarationErrorKind::EditableColumn`](super::DeclarationErrorKind::EditableColumn).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditableColumnFault {
    /// The resource has no form, so no edit route takes the cell's post.
    ///
    /// Give the resource a form, or show the field read-only.
    NoForm,
    /// The resource's record form binds no control for the field.
    ///
    /// Bind the field in the record form.
    Unplaced,
    /// The form edits the field with another control than the column's, carrying the one the
    /// column needs: a checkbox or switch for a `ToggleColumn`, a choice over static options for
    /// a `SelectColumn`, a one-line text field for a `TextInputColumn`.
    ///
    /// Edit the field with that control, or pick the column that matches the form's.
    OtherControl(&'static str),
    /// The form's control shows only while a condition holds, which reads fields the cell does
    /// not post.
    ///
    /// Show the field read-only.
    Conditional,
    /// The form's control is disabled on the edit form, so the edit route drops what it posts.
    ///
    /// Show the field read-only.
    Disabled,
}

impl EditableColumnFault {
    /// The sentence a mount error says after the column's name.
    pub(super) fn fmt(&self, f: &mut fmt::Formatter<'_>, column: &str) -> fmt::Result {
        match self {
            Self::NoForm => write!(
                f,
                "column '{column}' edits its field in place and the resource has no form to \
                 post it to: give the resource a form, or show the field read-only"
            ),
            Self::Unplaced => write!(
                f,
                "column '{column}' edits its field in place and the record form binds no control \
                 for it: bind the field in the record form"
            ),
            Self::OtherControl(control) => write!(
                f,
                "column '{column}' edits its field in place and the form edits it with another \
                 control than {control}: use that control, or the column matching the form's"
            ),
            Self::Conditional => write!(
                f,
                "column '{column}' edits its field in place and the form shows it only while a \
                 condition holds, which reads fields the cell does not post: show the field \
                 read-only"
            ),
            Self::Disabled => write!(
                f,
                "column '{column}' edits its field in place and the edit form disables it, so the \
                 save drops what the cell posts: show the field read-only"
            ),
        }
    }
}
