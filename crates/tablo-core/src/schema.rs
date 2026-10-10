//! Unified Schema primitive — fields and layout blocks that compose via `view!`.
//!
//! `Schema` holds layout blocks, embedded values, and [`Field`] slots and resolves every field once
//! into one list for rendering and validation.

mod condition;
pub(crate) mod embedded;
pub(crate) mod fields;
mod layouts;
mod lenses;
mod options;
pub(crate) mod relationship;
pub(crate) mod repeater;
pub(crate) mod tree;
pub(crate) mod validation;

use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
};

pub use condition::Watched;
pub use embedded::EmbeddedForm;
use fields::ChoiceControl;
pub use fields::{
    ChoiceField, CustomField, Field, FileField, IntoOptions, RepeaterField, TextField, Toggle,
};
pub(crate) use fields::{option_view, read_only, stored_upload, value_cell};
pub use layouts::{Grid, Group, Section};
pub(crate) use lenses::Binding;
pub use lenses::{FieldResolver, form_key};
pub use options::Options;
pub use relationship::MAX_RELATIONSHIP_OPTIONS;
pub(crate) use relationship::{OptionLoadError, OptionSource};
pub use repeater::{MAX_ROWS, RepeaterItem, parse_items, write_items};
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
        let source = source.watching(self.watched());
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

    /// Folds each repeater's posted rows into its own key, which
    /// [`parse_items`] reads.
    ///
    /// A resource's form and an action's input fold their submissions; a page handling its own
    /// post folds it before reading it, or the rows' keys stay apart.
    ///
    /// # Errors
    ///
    /// A repeater's key lists something other than its rows, a row posting nothing, or more than
    /// [`MAX_ROWS`] rows: no browser posts these, so a handler answers 400.
    pub fn fold_repeaters(
        &self,
        values: &mut HashMap<String, String>,
    ) -> std::result::Result<(), String> {
        for field in &self.fields {
            if let Some(repeater) = field.as_repeater() {
                repeater.fold(field.name(), values)?;
            }
        }
        Ok(())
    }

    /// Folds the values each multiple choice posted, listed in `lists`, into its one key: a list
    /// without blanks or repeats, which [`parse_list`](crate::form::parse_list) reads. A choice
    /// that posted nothing folds nothing, so an edit keeps the stored list.
    pub(crate) fn fold_choices(
        &self,
        values: &mut HashMap<String, String>,
        lists: &HashMap<String, Vec<String>>,
    ) {
        let multiple = self
            .fields
            .iter()
            .filter(|field| field.as_choice().is_some_and(ChoiceControl::is_multiple));
        for field in multiple {
            let Some(posted) = lists.get(field.name()) else {
                continue;
            };
            let mut keys: Vec<String> = Vec::new();
            for value in posted.iter().map(|value| value.trim()) {
                if !value.is_empty() && !keys.iter().any(|key| key == value) {
                    keys.push(value.to_string());
                }
            }
            values.insert(field.name().to_string(), crate::form::encode_list(&keys));
        }
    }

    /// A posted input as its parse reads it: the `reserved` keys the POST carries for itself
    /// dropped, each repeater's rows and each multiple choice's values folded into their one key
    /// (read with `parse_items` and `parse_list`), and a condition-hidden field's key removed,
    /// since the browser posts nothing for it.
    ///
    /// # Errors
    ///
    /// 400 for a malformed repeater and for a key no field declares.
    pub(crate) fn read_input(
        &self,
        values: &HashMap<String, String>,
        lists: &HashMap<String, Vec<String>>,
        reserved: &[&str],
    ) -> Result<HashMap<String, String>> {
        use topcoat::router::error::bad_request;

        let mut input = values.clone();
        input.retain(|key, _| !reserved.contains(&key.as_str()));
        self.fold_repeaters(&mut input).map_err(bad_request)?;
        self.fold_choices(&mut input, lists);
        let unknown = self.unknown_keys(&input);
        if !unknown.is_empty() {
            return Err(bad_request(format!("unknown field(s): {}", unknown.join(", "))).into());
        }
        for key in self.condition_hidden(&input) {
            input.remove(&key);
        }
        Ok(input)
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

    /// Returns the first key in `errors` that no control renders a message under: neither a
    /// field's own key nor, for a repeater, a row's key under `{key}.`.
    pub(crate) fn unplaced_error<'e>(&self, errors: &'e crate::FieldErrors) -> Option<&'e str> {
        errors.iter().map(|error| error.key.as_str()).find(|key| {
            !self
                .fields
                .iter()
                .any(|field| match key.strip_prefix(field.name()) {
                    Some(rest) => {
                        rest.is_empty() || (rest.starts_with('.') && field.as_repeater().is_some())
                    }
                    None => false,
                })
        })
    }

    /// One [`EmptyChoice`](crate::DeclarationErrorKind::EmptyChoice) per choice with neither
    /// options nor a relationship. Mounting checks a resource's form and its actions' inputs; a
    /// page's schema may build its options from data that is empty for now, so rendering does
    /// not.
    pub(crate) fn empty_choices(&self) -> Vec<crate::DeclarationErrorKind> {
        self.fields
            .iter()
            .filter(|field| field.offers_nothing())
            .map(|field| crate::DeclarationErrorKind::EmptyChoice {
                field: field.name().to_string(),
            })
            .collect()
    }

    /// What is wrong with the schema's dependent choices: a column of another model than the
    /// relationship's source, or no relationship, and a parent the schema does not place, which
    /// posts nothing to narrow the options by.
    fn dependent_errors(&self) -> Vec<crate::DeclarationErrorKind> {
        let mut errors = Vec::new();
        for field in &self.fields {
            let Some(choice) = field.as_choice() else {
                continue;
            };
            if choice.misdeclared_parent() {
                errors.push(crate::DeclarationErrorKind::MisdeclaredDependentChoice {
                    field: field.name().to_string(),
                });
            }
            let Some(parent) = choice.parent_key() else {
                continue;
            };
            match self.fields.iter().find(|placed| placed.name() == parent) {
                None => errors.push(crate::DeclarationErrorKind::UnplacedParentField {
                    field: field.name().to_string(),
                    parent: parent.to_string(),
                }),
                Some(parent)
                    if choice.is_multiple()
                        || parent.as_choice().is_some_and(ChoiceControl::is_multiple) =>
                {
                    errors.push(crate::DeclarationErrorKind::MultipleDependentChoice {
                        field: field.name().to_string(),
                    });
                }
                Some(_) => {}
            }
        }
        errors
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
        errors.extend(self.condition_errors());
        errors.extend(self.dependent_errors());
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
    /// variant it does not choose, and every field a condition hides.
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
            .chain(self.condition_hidden(values))
            .collect()
    }

    /// Adds to `errors` what the controls' own rules refuse in a submission: an email field's
    /// address, and a choice that is not one of its options. Skips hidden fields and keys
    /// `errors` already refuses.
    pub(crate) async fn check_controls(
        &self,
        cx: &Cx,
        values: &HashMap<String, String>,
        stored: &HashMap<String, String>,
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
            if let Some(repeater) = field.as_repeater() {
                repeater.check(cx, name, value, errors).await;
                continue;
            }
            if let Some(error) = field.check(value) {
                errors.push(error);
                continue;
            }
            let parent = field.parent_key().and_then(|key| values.get(key));
            for message in field
                .validate_exists(
                    cx,
                    value,
                    parent.map(String::as_str),
                    stored.get(name).map(String::as_str),
                )
                .await
            {
                errors.add(name, message);
            }
        }
    }

    /// What the controls' own rules refuse in `values`, alone.
    #[cfg(test)]
    pub(crate) async fn checked(&self, cx: &Cx, values: &HashMap<String, String>) -> FieldErrors {
        let mut errors = FieldErrors::new();
        self.check_controls(cx, values, &HashMap::new(), &mut errors)
            .await;
        errors
    }

    /// Re-checks every submitted relationship key through the write's open transaction, but a
    /// multiple choice's keys the record's stored value in `stored` holds already.
    pub(crate) async fn recheck_relationships(
        &self,
        cx: &Cx,
        values: &HashMap<String, String>,
        stored: &HashMap<String, String>,
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
            let parent = field.parent_key().and_then(|key| values.get(key));
            for message in field
                .recheck(
                    cx,
                    value,
                    parent.map(String::as_str),
                    stored.get(name).map(String::as_str),
                    &mut *ex,
                )
                .await
            {
                errors.add(name, message);
            }
        }
        errors
    }

    /// Puts back in each multiple choice's list the keys of the record's stored list `stored` it
    /// drops that the user cannot view, through the write's open transaction: the form never
    /// offered them, so its submission leaves them linked.
    pub(crate) async fn keep_unseen_links(
        &self,
        cx: &Cx,
        values: &mut HashMap<String, String>,
        stored: &HashMap<String, String>,
        ex: &mut dyn toasty::Executor,
    ) {
        for field in &self.fields {
            let name = field.name();
            let (Some(value), Some(held)) = (values.get(name), stored.get(name)) else {
                continue;
            };
            if let Some(kept) = field.keep_unseen(cx, value, held, &mut *ex).await {
                values.insert(name.to_string(), kept);
            }
        }
    }
}

#[cfg(test)]
mod tests;
