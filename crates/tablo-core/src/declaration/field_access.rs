//! The declaration mistakes of a resource's field access, and their wording.

use std::fmt;

/// Why a resource's [`field_access`](crate::ResourceDef::field_access) cannot hide its field
/// everywhere the resource shows it:
/// [`DeclarationErrorKind::FieldAccess`](super::DeclarationErrorKind::FieldAccess).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldAccessFault {
    /// Two declarations over one field.
    Duplicate,
    /// The field is the inverse side of a relation, a `#[has_many]` or a `#[has_one]`, whose
    /// records the related resource's relation table and policy show and link.
    Inverse,
    /// The field is the [soft-delete](crate::ResourceDef::soft_delete) column, whose state the
    /// trash control and the Restore buttons show every user.
    SoftDelete,
    /// The record title reads the field, and every page and relationship choice names a record
    /// by its title. Asked of a rule with a `view` rule only.
    RecordTitle,
    /// A form condition or a dependent choice watches the field, so what the form shows would
    /// tell what it holds. Asked of a rule with a `view` rule only.
    Watched,
    /// A column or a filter of a resource that restricts a field does not declare the fields it
    /// reads, so the panel cannot tell whether it shows the restricted one.
    UndeclaredReads {
        /// `"column"` or `"filter"`.
        what: &'static str,
        /// The column's or the filter's name.
        name: String,
    },
}

impl fmt::Display for FieldAccessFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate => f.write_str(
                "the field has two `field_access` declarations: declare it once, with both its \
                 `view` and its `edit` rule",
            ),
            Self::Inverse => f.write_str(
                "the field is a `has_many` or a `has_one`, whose records the relation table shows \
                 and links: restrict them with the related resource's policy, or drop the \
                 relation for the users who may not see it",
            ),
            Self::SoftDelete => f.write_str(
                "the field is the soft-delete column, whose state the trash control and the \
                 Restore and Delete permanently buttons show: restrict who restores and deletes \
                 with the policy's `Restore` and `ForceDelete` instead",
            ),
            Self::RecordTitle => f.write_str(
                "the record title reads the field, and every page and relationship choice names \
                 the record by its title: title the record by a field every user may view",
            ),
            Self::Watched => f.write_str(
                "a form condition or a dependent choice watches the field, so what the form shows \
                 would tell what the field holds: watch a field every user may view",
            ),
            Self::UndeclaredReads { what, name } => write!(
                f,
                "{what} `{name}` declares no fields it reads, so the panel cannot tell whether it \
                 shows a restricted one: declare them with `.reading(Reads::of(..))`, or \
                 implement `reads` on a custom {what}"
            ),
        }
    }
}
