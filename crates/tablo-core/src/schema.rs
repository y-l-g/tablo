//! Unified Schema primitive — fields and layout blocks that compose via `view!`.
//!
//! `Schema` holds layout blocks, embedded values, and [`Field`] slots and resolves every field once
//! into one list for rendering and validation.

pub(crate) mod embedded;
pub(crate) mod fields;
mod layouts;
mod lenses;
mod options;
pub(crate) mod relationship;
pub(crate) mod tree;
pub(crate) mod validation;

use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
};

pub use embedded::EmbeddedForm;
pub use fields::{ChoiceField, CustomField, Field, FileField, IntoOptions, TextField, Toggle};
pub(crate) use fields::{option_view, read_only, stored_upload, value_cell};
pub use layouts::{Grid, Group, Section};
pub(crate) use lenses::Binding;
pub use lenses::{FieldResolver, form_key};
pub use options::Options;
pub use relationship::MAX_RELATIONSHIP_OPTIONS;
pub(crate) use relationship::{OptionLoadError, OptionSource};
use topcoat::{Result, context::Cx, view::*};
pub use tree::{IntoSchema, Source};
pub(crate) use tree::{Node, Retype, render_nodes};
use tree::{bind_nodes, unbound_values};
pub(crate) use validation::TypedValue;

use crate::form::FieldErrors;

/// Composes fields and layout blocks and resolves every field once into one list.
///
/// `F` is the record form whose controls the schema places. A resource's form is a
/// `Schema<UserForm>`, which only `UserForm::controls()` fill, so a control from another form or a
/// field built with [`Field::text`] fails to compile there. A page's or an action's schema is a
/// plain `Schema`, which [`Field`]'s constructors fill.
///
/// ```compile_fail
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String, email: String }
/// # #[derive(tablo_core::RecordForm)]
/// # #[form(model = User)]
/// # struct UserForm { name: String }
/// use tablo_core::{Field, Schema};
///
/// let form: Schema<UserForm> = Schema::new((
///     UserForm::controls().name,
///     Field::text(User::fields().email()),
/// ));
/// ```
pub struct Schema<F = ()> {
    pub(crate) nodes: Vec<Node>,
    pub(crate) fields: Vec<Field>,
    form: PhantomData<fn() -> F>,
}

impl<F> Default for Schema<F> {
    fn default() -> Self {
        Self::from_parts(Vec::new(), Vec::new())
    }
}

impl<F> std::fmt::Debug for Schema<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Schema")
            .field("nodes", &self.nodes)
            .field("fields", &self.fields)
            .finish()
    }
}

impl Schema {
    /// A schema with no field: a page's or an action's that renders nothing. A resource's form
    /// that places nothing is `Schema::default()`, which renders every control.
    pub fn empty() -> Self {
        Self::default()
    }
}

impl<F> Retype for Schema<F> {
    type As<G> = Schema<G>;

    fn retype<G>(self) -> Schema<G> {
        Schema::from_parts(self.nodes, self.fields)
    }
}

impl<F> Schema<F> {
    pub(crate) fn from_parts(nodes: Vec<Node>, fields: Vec<Field>) -> Self {
        Self {
            nodes,
            fields,
            form: PhantomData,
        }
    }

    /// Reports whether this schema declares nothing to render.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Builds a `Schema` from any `IntoSchema` and reports duplicate field names as declaration
    /// errors.
    pub fn new(children: impl IntoSchema<F>) -> Self {
        children.into_schema()
    }

    /// Every field, in the order it joins the schema: a layout's fields as it composes, in
    /// declaration order, and an embedded value's fields when it binds.
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter()
    }

    /// Binds the schema's embedded values and embedded paths to `db`'s app schema.
    ///
    /// A panel binds the schemas it mounts; bind one a custom page renders before rendering it.
    /// A schema with no embedded value or path is bound from the start.
    pub fn bind(mut self, db: &toasty::Db) -> Self {
        self.bind_with(&FieldResolver::of_db(db));
        self
    }

    /// Binds the schema through `resolver`: builds each unbound embedded value in place, then
    /// binds every field's path.
    pub(crate) fn bind_with(&mut self, resolver: &FieldResolver) {
        bind_nodes(&mut self.nodes, &mut self.fields, resolver);
        for field in &self.fields {
            field.bind(resolver);
        }
    }

    /// Renders each control required exactly when `required` names its key.
    pub(crate) fn require(&mut self, required: &HashSet<&str>) {
        for field in &mut self.fields {
            let key = required.contains(field.name());
            field.set_required(key);
        }
    }

    /// Renders the schema from `source` and fails with declaration errors instead of rendering.
    ///
    /// # Errors
    ///
    /// A misdeclared schema fails with its errors rather than render.
    pub async fn render<'a>(&self, cx: &'a Cx, source: Source<'_>) -> Result<BoxView<'a>> {
        let errors = self.declaration_errors();
        if !errors.is_empty() {
            return Err(crate::error::misdeclared(&errors));
        }
        render_nodes(cx, &self.nodes, &self.fields, &source).await
    }

    /// Appends `other`'s nodes and fields after this one's, re-slotting its field slots.
    pub(crate) fn append(&mut self, other: Schema<F>) {
        let Schema {
            mut nodes, fields, ..
        } = other;
        let offset = self.fields.len();
        for node in &mut nodes {
            node.offset(offset);
        }
        self.fields.extend(fields);
        self.nodes.extend(nodes);
    }

    /// The embedded node a derived value's schema holds.
    pub(crate) fn embedded_root(&self) -> &embedded::Embedded {
        match self.nodes.as_slice() {
            [Node::Embedded(node)] => node,
            _ => panic!("an embedded value's schema is its one embedded node"),
        }
    }

    /// Appends another schema's nodes after this one's.
    pub fn extend(mut self, other: Schema<F>) -> Schema<F> {
        self.append(other);
        self
    }

    /// Lists keys in `values` that no declared input owns, sorted, so handlers reject
    /// client-controlled writes.
    pub(crate) fn unknown_keys(&self, values: &HashMap<String, String>) -> Vec<String> {
        let known: HashSet<&str> = self.fields.iter().map(Field::name).collect();
        let mut out: Vec<String> = values
            .keys()
            .filter(|k| !known.contains(k.as_str()))
            .cloned()
            .collect();
        out.sort();
        out
    }

    /// Reports what is wrong with this declaration: an embedded value or path never bound, a field
    /// whose lens binds no single column, and two fields sharing a name.
    pub fn declaration_errors(&self) -> Vec<crate::DeclarationErrorKind> {
        let mut unbound = Vec::new();
        unbound_values(&self.nodes, &mut unbound);
        let mut errors: Vec<_> = unbound
            .into_iter()
            .map(|item| crate::DeclarationErrorKind::Unbound { item })
            .collect();
        let mut seen = HashSet::new();
        for field in &self.fields {
            match field.misdeclared() {
                Some(error) => errors.push(error),
                None if !seen.insert(field.name()) => {
                    errors.push(crate::DeclarationErrorKind::DuplicateField {
                        name: field.name().to_string(),
                    });
                }
                None => {}
            }
        }
        errors
    }

    /// Collects the keys of the fields this submission hides: the payload of every embedded
    /// variant it does not choose.
    pub(crate) fn hidden_fields(&self, values: &HashMap<String, String>) -> HashSet<String> {
        fn walk(nodes: &[Node], values: &HashMap<String, String>, out: &mut Vec<usize>) {
            for node in nodes {
                match node {
                    Node::Embedded(embedded) => embedded.hidden_fields(values, out),
                    node => {
                        if let Some(children) = node.children() {
                            walk(children, values, out);
                        }
                    }
                }
            }
        }
        let mut indices = Vec::new();
        walk(&self.nodes, values, &mut indices);
        indices
            .into_iter()
            .map(|index| self.fields[index].name().to_string())
            .collect()
    }

    /// Adds to `errors` what the controls' own rules refuse in a submission: an email field's
    /// address, and a choice that is not one of its options. Skips hidden fields and keys
    /// `errors` already refuses.
    pub(crate) async fn check_controls(
        &self,
        cx: &Cx,
        values: &HashMap<String, String>,
        errors: &mut FieldErrors,
    ) {
        let hidden = self.hidden_fields(values);
        for field in &self.fields {
            let name = field.name();
            if errors.contains_key(name) || hidden.contains(name) {
                continue;
            }
            let Some(value) = values.get(name) else {
                continue;
            };
            if let Some(error) = field.check(value) {
                errors.push(error);
                continue;
            }
            for message in field.validate_exists(cx, value).await {
                errors.add(name, message);
            }
        }
    }

    /// What the controls' own rules refuse in `values`, alone.
    #[cfg(test)]
    pub(crate) async fn checked(&self, cx: &Cx, values: &HashMap<String, String>) -> FieldErrors {
        let mut errors = FieldErrors::new();
        self.check_controls(cx, values, &mut errors).await;
        errors
    }

    /// Re-checks every submitted relationship key through the write's open transaction.
    pub(crate) async fn recheck_relationships(
        &self,
        cx: &Cx,
        values: &HashMap<String, String>,
        ex: &mut dyn toasty::Executor,
    ) -> FieldErrors {
        let mut errors = FieldErrors::new();
        let hidden = self.hidden_fields(values);
        for field in &self.fields {
            let name = field.name();
            if hidden.contains(name) {
                continue;
            }
            let Some(value) = values.get(name) else {
                continue;
            };
            for message in field.recheck(cx, value, &mut *ex).await {
                errors.add(name, message);
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests;
