//! The declaration mistakes of a many-to-many field or relation, and their wording.

use std::fmt;

/// Why a many-to-many field or relation cannot link records:
/// [`DeclarationErrorKind::ManyToMany`](super::DeclarationErrorKind::ManyToMany).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ManyToManyFault {
    /// The field is no `#[has_many(via = joins.target)]` stepping through a `has_many` to a join
    /// model, then the join model's `belongs_to` to the target.
    ///
    /// Declare it through a `has_many` to the join model and the join model's `belongs_to`.
    NotJoinModel,
    /// A foreign key of the join model spans several columns, or references no single-column
    /// primary key.
    ///
    /// Key each side by one column.
    CompositeKey,
    /// The join model has a column a new link cannot fill: not nullable, not `#[auto]`, and not
    /// one of its two foreign keys.
    ///
    /// Make it nullable or `#[auto]`, or link the records in your own record fn.
    UnwritableColumn {
        /// The join model.
        model: String,
        /// The column.
        column: String,
    },
    /// The form's control for the field is not a multiple choice over a relationship.
    ///
    /// Declare `.choice().relationship::<R>().multiple()`.
    NotMultipleChoice,
    /// The relationship's records are not the target's.
    ///
    /// Point the relationship at the target's resource.
    OtherSource,
    /// A multiple choice in a resource's form over no many-to-many field: a relationship's keys,
    /// which no column stores, or static options over a column holding one value.
    ///
    /// Bind a relationship to a `#[has_many(via = ..)]` field, and static options to a `Vec`.
    NoJoin,
}

/// [`DeclarationErrorKind::ManyToMany`](super::DeclarationErrorKind::ManyToMany)'s sentence.
pub(super) fn fmt_many_to_many(
    f: &mut fmt::Formatter<'_>,
    field: &str,
    fault: &ManyToManyFault,
) -> fmt::Result {
    match fault {
        ManyToManyFault::NotJoinModel => write!(
            f,
            "`{field}` is no many-to-many field: declare it \
             `#[has_many(via = joins.target)]`, through a `has_many` to a join model and \
             the join model's `belongs_to` to the target"
        ),
        ManyToManyFault::CompositeKey => write!(
            f,
            "`{field}`'s join model holds a key of several columns, or one referencing no \
             single-column primary key: a link writes one column per side"
        ),
        ManyToManyFault::UnwritableColumn { model, column } => write!(
            f,
            "`{field}`'s join model `{model}` has a column `{column}` a new link cannot \
             fill: make it nullable or `#[auto]`, or link the records in your own record fn"
        ),
        ManyToManyFault::NotMultipleChoice => write!(
            f,
            "`{field}` is a many-to-many field, so its control is a multiple choice over \
             a relationship: `.choice().relationship::<R>().multiple()`"
        ),
        ManyToManyFault::OtherSource => write!(
            f,
            "`{field}` offers another model's records than the ones its join model links"
        ),
        ManyToManyFault::NoJoin => write!(
            f,
            "multiple choice `{field}` names neither a many-to-many field of the model nor a \
             list column, so nothing would store its choices: bind a relationship to a \
             `#[has_many(via = ..)]` field, and static options to a `Vec`"
        ),
    }
}
