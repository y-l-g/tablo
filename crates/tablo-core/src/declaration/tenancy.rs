//! The declaration mistakes of a resource's tenancy, and their wording.

use std::fmt;

/// Why a resource's tenancy cannot scope its rows:
/// [`DeclarationErrorKind::Tenancy`](super::DeclarationErrorKind::Tenancy).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TenancyFault {
    /// No tenancy declared over a model with a `TenantId` column, which would serve every
    /// tenant's rows.
    Undeclared {
        /// The model's tenant column.
        column: &'static str,
    },
    /// A `Tenancy::column` lens that names no field of the model.
    ColumnNotAField,
    /// A `Tenancy::via` lens that names one field of the model.
    ViaOwnColumn,
    /// A `Tenancy::via` lens whose first step is no `belongs_to` relation.
    ViaWithoutBelongsTo,
    /// A `Tenancy::via` resource whose form writes the parent's foreign key other than through a
    /// relationship field over a tenant-scoped resource of the parent's model.
    UnguardedForeignKey {
        /// The `belongs_to` relation the lens steps through.
        relation: String,
        /// The foreign-key columns written another way.
        keys: Vec<String>,
    },
}

impl fmt::Display for TenancyFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undeclared { column } => write!(
                f,
                "the model has the tenant column `{column}` and the resource declares no \
                 tenancy, so it would serve every tenant's rows: declare \
                 `Tenancy::column(lens!(Model.{column}))`, or `Tenancy::none()` to serve every \
                 tenant on purpose"
            ),
            Self::ColumnNotAField => f.write_str(
                "the `Tenancy::column` lens names no field of the model: name a field typed \
                 `TenantId`, or use `Tenancy::via` for a tenant reached through a relation",
            ),
            Self::ViaOwnColumn => f.write_str(
                "the `Tenancy::via` lens names one field of the model: use `Tenancy::column` for \
                 the model's own tenant column",
            ),
            Self::ViaWithoutBelongsTo => f.write_str(
                "the `Tenancy::via` lens does not start at a `belongs_to` relation: the tenant \
                 must be the parent's its foreign key names",
            ),
            Self::UnguardedForeignKey { relation, keys } => write!(
                f,
                "foreign key `{}` is not written through a relationship field over a \
                 tenant-scoped resource of `{relation}`'s model: the write re-checks only such a \
                 field's key, so any other could attach the row to another tenant's parent",
                keys.join("`, `")
            ),
        }
    }
}
