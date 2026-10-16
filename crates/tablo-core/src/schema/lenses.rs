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
            }),
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
pub(crate) fn resolve_enum<M, T>(path: &Path<M, T>) -> Option<EnumShape>
where
    M: toasty::schema::Model,
{
    model::embedded_enum(&AppSchema::of_model::<M>().ok()?, &ModelPath::of(path))
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
