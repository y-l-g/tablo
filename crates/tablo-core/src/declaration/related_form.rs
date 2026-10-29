//! The declaration mistakes of a `has_one` relation's related form, and their wording.

use std::fmt;

/// Why a [`Relation::has_one`](crate::Relation::has_one) cannot render its related resource's
/// form inside the owner's:
/// [`DeclarationErrorKind::RelatedForm`](super::DeclarationErrorKind::RelatedForm).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RelatedFormFault {
    /// The owner has no form for the related form to render in.
    ///
    /// Give the owner a form, or declare a `has_many` relation, which renders a table.
    NoOwnerForm,
    /// The related resource has no form.
    ///
    /// Give it a form.
    NoForm,
    /// The related resource's form binds no control for the foreign key, which the owner's key
    /// fills.
    ///
    /// Bind the foreign key in its form.
    UnboundForeignKey {
        /// The foreign key.
        column: String,
    },
    /// The related resource soft-deletes: a trashed child would stay the record's while the form
    /// created another.
    ///
    /// Declare a `has_many` relation, whose table offers the trashed records.
    SoftDeletes,
    /// The related form holds a control that posts keys besides its own or fetches from its
    /// resource's routes: a file field, a repeater, an embedded value, a searchable or dependent
    /// choice.
    ///
    /// Leave the field out of the related resource's form, or declare a `has_many` relation.
    OwnKeys {
        /// The field.
        field: String,
    },
    /// An owner form key starts with the related form's prefix, the related resource's slug and
    /// a dot, under which the related form's controls post.
    ///
    /// Rename the field, or give the related resource another slug.
    ShadowedKey {
        /// The owner's form key.
        field: String,
    },
}

impl RelatedFormFault {
    /// The sentence a mount error says.
    pub(super) fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOwnerForm => f.write_str(
                "a `has_one` relation renders its related form inside the owner's form, and the \
                 owner has none: give it a form, or declare a `has_many` relation",
            ),
            Self::NoForm => f.write_str(
                "a `has_one` relation renders the related resource's form, and it has none: \
                 give it a form",
            ),
            Self::UnboundForeignKey { column } => write!(
                f,
                "the related resource's form binds no control for `{column}`, which the owner's \
                 key fills: bind it in that form"
            ),
            Self::SoftDeletes => f.write_str(
                "the related resource soft-deletes, so a trashed child would stay the record's \
                 while its form created another: declare a `has_many` relation",
            ),
            Self::OwnKeys { field } => write!(
                f,
                "the related form's field `{field}` posts keys besides its own or fetches from \
                 its resource's routes, which a form inside another cannot: leave it out of \
                 the related resource's form, or declare a `has_many` relation"
            ),
            Self::ShadowedKey { field } => write!(
                f,
                "the owner's form key `{field}` starts with the related form's prefix: rename \
                 it, or give the related resource another slug"
            ),
        }
    }
}
