//! Resolves typed Toasty paths to form keys and field metadata.
//!
//! A single-field path resolves against its own model when its declaration is built. An embedded
//! path names a flattened storage column only the app schema knows, so it stays unbound until its
//! declaration binds to a [`FieldResolver`]: the panel binds every declaration it mounts, and
//! [`Schema::bind`](crate::Schema::bind) and [`Table::bind`](crate::Table::bind) bind one built
//! outside a panel.

use std::sync::OnceLock;

use toasty::stmt::Path;
use topcoat::context::Cx;

pub(crate) use crate::toasty_compat::model::{EnumShape, LeafField};
use crate::{
    DeclarationErrorKind,
    toasty_compat::model::{self, AppSchema, ModelPath},
};

/// A path bound to the column it names, with the metadata a declaration defaults from.
///
/// A single-field path binds when it is built. An embedded path binds when its declaration binds
/// ([`Self::bind`]); until then it names a placeholder spelling its steps, which reads as no other
/// field's duplicate, and reports [`DeclarationErrorKind::Unbound`].
///
/// A column reports its nullability, and defaults `unique` from a single- or multi-field unique
/// index it belongs to. An embedded leaf is never unique by default and reports nullable: only the
/// matching enum variant writes a variant payload column. That is the binding default, not a
/// storage fact — the flattened column of a required embedded struct is `NOT NULL`.
#[derive(Debug, Clone)]
pub(crate) struct Binding {
    /// The path, or `None` for a key no path names (an embedded enum's discriminant).
    path: Option<ModelPath>,
    model: &'static str,
    placeholder: String,
    leaf: OnceLock<Result<LeafField, DeclarationErrorKind>>,
}

impl Binding {
    /// Bind `path`, now when it names one field of its model and at [`Self::bind`] when it is
    /// embedded.
    pub(crate) fn of<M, T>(path: &Path<M, T>) -> Self
    where
        M: toasty::schema::Model,
    {
        let path = ModelPath::of(path);
        let leaf = OnceLock::new();
        if !path.is_embedded() {
            let _ = leaf.set(model::single_field::<M>(&path));
        }
        Self {
            placeholder: format!("{:?}", path.steps()),
            path: Some(path),
            model: std::any::type_name::<M>(),
            leaf,
        }
    }

    /// A key no path names, bound as given.
    pub(crate) fn named(name: String, label: String) -> Self {
        Self {
            path: None,
            model: "",
            placeholder: String::new(),
            leaf: OnceLock::from(Ok(LeafField {
                name,
                label,
                nullable: true,
                unique: false,
            })),
        }
    }

    /// Bind an embedded path through `resolver`'s app schema; a bound path stays as it is.
    pub(crate) fn bind(&self, resolver: &FieldResolver) {
        if let Some(path) = &self.path {
            self.leaf
                .get_or_init(|| resolver.resolve_embedded(path, self.model));
        }
    }

    fn leaf(&self) -> Option<&LeafField> {
        self.leaf.get().and_then(|leaf| leaf.as_ref().ok())
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
        match self.leaf.get() {
            Some(Ok(_)) => None,
            Some(Err(error)) => Some(error.clone()),
            None => Some(DeclarationErrorKind::Unbound { item: self.model }),
        }
    }
}

/// The form key a record-form field binds: the column `path` resolves to.
#[doc(hidden)]
pub fn form_key<M, T>(path: impl Into<Path<M, T>>) -> String
where
    M: toasty::schema::Model,
{
    Binding::of(&path.into()).name().to_string()
}

/// The app schema embedded paths resolve through: the compiled schema of the app's `Db`.
///
/// A panel binds the declarations it mounts through its `Db`'s, and hands one to
/// [`RecordForm::fields`](crate::RecordForm::fields) and
/// [`EmbeddedForm`](crate::EmbeddedForm)'s schema builder. A hand-written impl passes it on to
/// the embedded values it builds.
#[derive(Clone, Default)]
pub struct FieldResolver {
    schema: Option<AppSchema>,
}

impl FieldResolver {
    pub(crate) fn new(schema: Option<AppSchema>) -> Self {
        Self { schema }
    }

    /// The resolver over `db`'s app schema.
    pub(crate) fn of_db(db: &toasty::Db) -> Self {
        Self::new(Some(AppSchema::of_db(db)))
    }

    /// The resolver over the app schema of the `Db` the request carries, if any.
    pub(crate) fn of(cx: &Cx) -> Self {
        Self::new(AppSchema::of(cx))
    }

    /// Whether an app schema backs this resolver at all.
    ///
    /// An embedded enum has no fallback without one — its discriminant column and variants come
    /// from the schema — so its entry point says so rather than reporting a traversal-lens error.
    pub(crate) fn has_schema(&self) -> bool {
        self.schema.is_some()
    }

    /// Resolve a lens to its leaf field as a declaration binding it would: a single field against
    /// its model, an embedded path through the app schema.
    ///
    /// # Errors
    ///
    /// A lens that resolves to no single column.
    #[cfg(test)]
    pub(crate) fn resolve<M, T>(&self, path: Path<M, T>) -> Result<LeafField, DeclarationErrorKind>
    where
        M: toasty::schema::Model,
    {
        let path = ModelPath::of(&path);
        if path.is_embedded() {
            self.resolve_embedded(&path, std::any::type_name::<M>())
        } else {
            model::single_field::<M>(&path)
        }
    }

    /// Resolve an embedded path through the app schema; without one, the path is a traversal the
    /// single-field rule refuses, since binding its first segment misbinds in release.
    fn resolve_embedded(
        &self,
        path: &ModelPath,
        model: &'static str,
    ) -> Result<LeafField, DeclarationErrorKind> {
        model::embedded_leaf(self.schema.as_ref(), path, model)
    }

    /// The discriminant column and variants of the embedded **enum** at `path`, or `None` when
    /// `path` names anything else or no app schema backs the resolver.
    ///
    /// `path` addresses the embedded field itself (`Post::fields().publication()`), not one of
    /// its leaves.
    pub(crate) fn resolve_enum<M, T>(&self, path: Path<M, T>) -> Option<EnumShape> {
        model::embedded_enum(self.schema.as_ref()?, &ModelPath::of(&path))
    }
}

#[cfg(test)]
mod tests;
