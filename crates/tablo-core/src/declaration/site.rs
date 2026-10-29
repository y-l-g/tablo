//! [`Site`]: where in a declaration a mistake is.

use std::fmt;

/// The part of a declaration a [`DeclarationError`](super::DeclarationError) is in.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Site {
    /// The panel's own configuration: its prefix, served directories, assets and auth.
    Panel,
    /// The resource or page itself: its registration, slug, actions or create columns, or a
    /// second home page.
    Registration,
    /// `ResourceDef::tenancy`.
    Tenancy,
    /// `ResourceDef::soft_delete`.
    SoftDelete,
    /// The `ResourceDef::field_access` over this field.
    FieldAccess(String),
    /// `ResourceDef::record_title`.
    RecordTitle,
    /// `ResourceDef::table`.
    Table,
    /// `ResourceDef::form`, checked against the record form.
    Form,
    /// `ResourceDef::detail`.
    Detail,
    /// The `ResourceDef::relation` to the resource with this slug, or this type when the panel
    /// does not register it.
    Relation(String),
}

/// How an error message names the site after its resource: nothing for the resource itself.
impl fmt::Display for Site {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Panel | Self::Registration => Ok(()),
            Self::Tenancy => f.write_str(" tenancy"),
            Self::SoftDelete => f.write_str(" soft delete"),
            Self::FieldAccess(field) => write!(f, " field access `{field}`"),
            Self::RecordTitle => f.write_str(" record title"),
            Self::Table => f.write_str(" table"),
            Self::Form => f.write_str(" form"),
            Self::Detail => f.write_str(" detail"),
            Self::Relation(key) => write!(f, " relation `{key}`"),
        }
    }
}
