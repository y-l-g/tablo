//! The declaration mistakes of a resource's field access, and their wording.

use std::fmt;

/// Why a [`field_access`](crate::ResourceDef::field_access) cannot hide its field everywhere the
/// panel shows it:
/// [`DeclarationErrorKind::FieldAccess`](super::DeclarationErrorKind::FieldAccess).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldAccessFault {
    /// Two declarations over one field.
    ///
    /// Declare it once, with both its `view` and its `edit` rule.
    Duplicate,
    /// The field is the inverse side of a relation, a `#[has_many]` or a `#[has_one]`, whose
    /// records the related resource's relation table and policy show and link.
    ///
    /// Restrict them with the related resource's policy.
    Inverse,
    /// The field is the [soft-delete](crate::ResourceDef::soft_delete) column, whose state the
    /// trash control and the Restore buttons show every user.
    ///
    /// Restrict restores and force deletes with the policy's `Restore` and `ForceDelete`.
    SoftDelete,
    /// The record title reads the field, and every page and relationship choice names a record
    /// by its title. Asked of a rule with a `view` rule only.
    ///
    /// Title the record by a field every user may view.
    RecordTitle,
    /// A rule hides a primary key. The key is every record's URL id and its default label, which
    /// every page and relationship choice shows, so no user may be kept from seeing it.
    ///
    /// Restrict a field that is not the key.
    RecordKey,
    /// A form condition, a dependent choice or another field's comparison watches the field, so
    /// what the form shows or refuses would tell what it holds. Asked of a rule with a `view`
    /// rule only.
    ///
    /// Watch or compare with a field every user may view.
    Watched,
    /// A column or a filter of a resource over a model a panel restricts a field of does not
    /// declare the fields it reads, so the panel cannot tell whether it shows the restricted one.
    ///
    /// Declare `.reading(Reads::of(..))` on a `ComputedColumn`, a `QueryFilter` or a described
    /// `TextColumn`, or implement `reads` on a custom column or filter.
    UndeclaredReads {
        /// `"column"` or `"filter"`.
        what: &'static str,
        /// The column's or the filter's name.
        name: String,
    },
    /// A column or a filter reads records of a model a mounted resource restricts a field of,
    /// under no field access a panel can mask.
    ///
    /// Show the records through a mounted resource: `RelationColumn::new::<R>` naming the fields it
    /// reads, `RelationColumn::of::<R>` for their title, or a relationship filter over the foreign
    /// key.
    UnmaskedRelation {
        /// The column's or the filter's name.
        name: String,
        /// The related model's name.
        model: String,
    },
    /// A column or a filter reads a relation the mount cannot read off the model's schema, so it
    /// cannot tell whether the relation reaches records a mounted resource restricts.
    ///
    /// Declare the relation as a `Deferred` field of the model.
    UnresolvedRelation {
        /// The column's or the filter's name.
        name: String,
    },
    /// A dependent choice narrows its options by a field a mounted resource restricts, so the
    /// options would show which records hold each value of it.
    ///
    /// Narrow by a field every user may view.
    NarrowedBy {
        /// The field the choice narrows by.
        field: String,
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
            Self::RecordKey => f.write_str(
                "the field is a primary key, which every URL and relationship choice names a record by: \
                 restrict a field that is not the key"
            ),
            Self::Watched => f.write_str(
                "a form condition, a dependent choice or a comparison watches the field, so what \
                 the form shows or refuses would tell what the field holds: watch or compare with \
                 a field every user may view",
            ),
            Self::UndeclaredReads { what, name } => write!(
                f,
                "{what} `{name}` declares no fields it reads, so the panel cannot tell whether it \
                 shows a restricted one: declare them with `.reading(Reads::of(..))`, or \
                 implement `reads` on a custom {what}"
            ),
            Self::UnmaskedRelation { name, model } => write!(
                f,
                "`{name}` reads `{model}` records under no mounted resource's field access, and a \
                 mounted resource restricts a field of `{model}`: read them through a resource, \
                 with `RelationColumn::new::<R>` naming the fields it reads or `RelationColumn::of::<R>` \
                 for their title, and keep a computed or custom column, a text column's closure or a row \
                 color off a page that loads them"
            ),
            Self::NarrowedBy { field } => write!(
                f,
                "a dependent choice narrows its options by `{field}`, which a mounted resource \
                 restricts, so the options would show which records hold each value of it: narrow \
                 by a field every user may view"
            ),
            Self::UnresolvedRelation { name } => write!(
                f,
                "`{name}` reads a relation the mount cannot read off the model's schema, so it \
                 cannot show that no restricted record reaches it"
            ),
        }
    }
}
