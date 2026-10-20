//! Resolves typed Toasty paths to form keys and field metadata.
//!
//! A path resolves when its declaration is built: a single field against its own model, an
//! embedded path against the schema a `Db` registering its model compiles, which names the
//! flattened column the app stores it in.

use toasty::stmt::Path;

pub(crate) use crate::toasty_compat::model::{EnumShape, LeafField};
use crate::{
    DeclarationErrorKind,
    toasty_compat::model::{self, AppSchema, ModelPath},
};

/// A path bound to the column it names, with the metadata a declaration defaults from.
///
/// A path that names no single column keeps a placeholder spelling its steps, which reads as no
/// other field's duplicate, and reports why ([`Self::misdeclared`]).
///
/// A column reports its nullability, and defaults `unique` from a single- or multi-field unique
/// index it belongs to. An embedded leaf is never unique by default and reports nullable: only the
/// matching enum variant writes a variant payload column. That is the binding default, not a
/// storage fact — the flattened column of a required embedded struct is `NOT NULL`.
#[derive(Debug, Clone)]
pub(crate) struct Binding {
    placeholder: String,
    leaf: Result<LeafField, DeclarationErrorKind>,
    /// The model's root field the path starts at; `None` for a key no path names.
    root: Option<String>,
}

impl Binding {
    /// Bind `path` to the column it names.
    pub(crate) fn of<M, T>(path: &Path<M, T>) -> Self
    where
        M: toasty::schema::Model,
    {
        let path = ModelPath::of(path);
        Self {
            placeholder: format!("{:?}", path.steps()),
            leaf: resolve::<M>(&path),
            root: model::root_field::<M>(&path),
        }
    }

    /// `path`, which binds no column for `error`.
    pub(crate) fn refused<M, T>(path: &Path<M, T>, error: DeclarationErrorKind) -> Self {
        Self {
            placeholder: format!("{:?}", ModelPath::of(path).steps()),
            leaf: Err(error),
            root: None,
        }
    }

    /// A key no path names, bound as given.
    pub(crate) fn named(name: String, label: String) -> Self {
        Self {
            placeholder: String::new(),
            leaf: Ok(LeafField {
                name,
                label,
                nullable: true,
                unique: false,
                unique_with: Vec::new(),
            }),
            root: None,
        }
    }

    fn leaf(&self) -> Option<&LeafField> {
        self.leaf.as_ref().ok()
    }

    /// The form key: the storage column the path names.
    pub(crate) fn name(&self) -> &str {
        self.leaf().map_or(&self.placeholder, |leaf| &leaf.name)
    }

    /// The label a declaration defaults to.
    pub(crate) fn label(&self) -> &str {
        self.leaf().map_or("", |leaf| &leaf.label)
    }

    pub(crate) fn nullable(&self) -> bool {
        self.leaf().is_none_or(|leaf| leaf.nullable)
    }

    pub(crate) fn unique(&self) -> bool {
        self.leaf().is_some_and(|leaf| leaf.unique)
    }

    /// Each unique index covering the column, as the names of its other fields.
    pub(crate) fn unique_with(&self) -> &[Vec<String>] {
        self.leaf().map_or(&[], |leaf| &leaf.unique_with)
    }

    /// The model's root field the path reads, as a [`Reads`](crate::lens::Reads).
    pub(crate) fn reads<M>(&self) -> crate::lens::Reads<M> {
        match &self.root {
            Some(root) => crate::lens::Reads::nothing().named(root.clone()),
            None => crate::lens::Reads::nothing(),
        }
    }

    /// Why the path binds no column, when it does not.
    pub(crate) fn misdeclared(&self) -> Option<DeclarationErrorKind> {
        self.leaf.as_ref().err().cloned()
    }
}

/// The leaf field `path` of `M` names: a single field against `M` alone, an embedded path
/// through the schema `M` compiles to.
///
/// # Errors
///
/// A path that reaches no single column, or a model no driver compiles.
pub(crate) fn resolve<M>(path: &ModelPath) -> Result<LeafField, DeclarationErrorKind>
where
    M: toasty::schema::Model,
{
    let model = std::any::type_name::<M>();
    if !path.is_embedded() {
        return model::single_field::<M>(path);
    }
    let schema = AppSchema::of_model::<M>()
        .map_err(|error| DeclarationErrorKind::Uncompiled { model, error })?;
    model::embedded_leaf(&schema, path, model)
}

/// The discriminant column and variants of the embedded **enum** at `path`, or `None` when
/// `path` names anything else.
///
/// `path` addresses the embedded field itself (`Post::fields().publication()`), not one of its
/// leaves.
///
/// # Errors
///
/// A model no driver compiles.
pub(crate) fn resolve_enum<M, T>(
    path: &Path<M, T>,
) -> Result<Option<EnumShape>, DeclarationErrorKind>
where
    M: toasty::schema::Model,
{
    let model = std::any::type_name::<M>();
    let schema = AppSchema::of_model::<M>()
        .map_err(|error| DeclarationErrorKind::Uncompiled { model, error })?;
    Ok(model::embedded_enum(&schema, &ModelPath::of(path)))
}

/// The form key a record-form field binds: the column `path` resolves to.
#[doc(hidden)]
pub fn form_key<M, T>(path: impl Into<Path<M, T>>) -> String
where
    M: toasty::schema::Model,
{
    Binding::of(&path.into()).name().to_string()
}

#[cfg(test)]
mod tests;
