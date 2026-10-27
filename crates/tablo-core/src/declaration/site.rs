//! [`Site`]: where in a declaration a mistake is.

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
