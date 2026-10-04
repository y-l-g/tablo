//! Unified Schema primitive — fields and layout blocks that compose via `view!`.
//!
//! `Schema` holds layout blocks, embedded values, and [`Field`] slots and resolves every field once
//! into one list for rendering and validation.
//!
//! `lens_field` reaches into `toasty_core` (upstream issue #114); retire the walk when Toasty
//! exposes it (upstream #183).

pub(crate) mod embedded;
mod fields;
mod layouts;
mod lenses;
mod options;
mod relationship;
mod tree;
mod validation;

use std::collections::{HashMap, HashSet};

pub use embedded::EmbeddedForm;
pub use fields::{
    ChoiceField, Control, ControlInput, CustomField, Field, FileField, IntoOptions, TextField,
    Toggle,
};
pub(crate) use fields::{model_name, option_view};
pub use layouts::{Grid, Group, Repeater, Section};
pub use lenses::{DeclCx, ResolvedLens};
pub(crate) use lenses::{LensBinding, capitalize, lens_field, lens_field_unique};
pub use options::Options;
pub(crate) use relationship::OptionLoadError;
pub use relationship::{MAX_RELATIONSHIP_OPTIONS, OptionSource};
use topcoat::{Result, context::Cx, view::*};
pub use tree::{IntoSchema, Source};
pub(crate) use tree::{LeafPlace, Node, render_nodes, walk_absent_groups};
pub use validation::TypedValue;
pub(crate) use validation::required_error;

use crate::form::FieldErrors;

/// One control and the rule an empty submission meets.
#[derive(Debug, Clone)]
pub(crate) struct ControlCheck {
    /// The key the control posts.
    pub(crate) name: String,
    /// Whether an empty submission fails the control's rules.
    pub(crate) required: bool,
    /// The message an empty submission produces, when it fails.
    pub(crate) required_error: Option<String>,
    /// Whether the control sits inside a `Repeater`.
    pub(crate) in_repeater: bool,
    /// Where the control sits in the form.
    pub(crate) place: LeafPlace,
}

impl ControlCheck {
    /// Reports whether an empty submission reaches this control's rule.
    pub(crate) fn needs_answer(&self) -> bool {
        match self.place {
            LeafPlace::Rendered => true,
            LeafPlace::Payload => self.in_repeater,
            LeafPlace::Discriminant => false,
        }
    }
}

/// Composes fields and layout blocks and resolves every field once into one list.
#[derive(Debug, Default)]
pub struct Schema {
    pub(crate) nodes: Vec<Node>,
    pub(crate) fields: Vec<Field>,
}

impl Schema {
    /// Reports whether this schema declares nothing to render.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Builds a `Schema` from any `IntoSchema` and reports duplicate field names as declaration
    /// errors.
    pub fn new(children: impl IntoSchema) -> Self {
        children.into_schema()
    }

    pub fn empty() -> Self {
        Self::default()
    }

    /// Every field, in declaration order.
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter()
    }

    /// Renders the schema from `source` and fails with declaration errors instead of rendering.
    ///
    /// # Errors
    ///
    /// A misdeclared schema fails with its errors rather than render.
    pub async fn render<'a>(&self, cx: &'a Cx, source: Source<'_>) -> Result<BoxView<'a>> {
        let errors = self.declaration_errors();
        if !errors.is_empty() {
            return Err(crate::error::TabloError::Declaration(errors.join("; ")).into());
        }
        render_nodes(cx, &self.nodes, &self.fields, &source).await
    }

    /// Appends `other`'s nodes and fields after this one's, re-slotting its field slots.
    pub(crate) fn append(&mut self, other: Schema) {
        let Schema { mut nodes, fields } = other;
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

    /// Rewrites submitted values into their fields' stored spelling and leaves empty submissions
    /// empty for the presence rule to refuse.
    pub fn normalize_values(&self, values: &mut HashMap<String, String>) {
        for field in &self.fields {
            let Some(submitted) = values.get_mut(field.name()) else {
                continue;
            };
            if submitted.trim().is_empty() {
                continue;
            }
            if let Ok(normalized) = field.normalize(submitted) {
                *submitted = normalized;
            }
        }
    }

    /// Appends another schema's nodes after this one's.
    pub fn extend(mut self, other: Schema) -> Schema {
        self.append(other);
        self
    }

    /// Collects every field with the rule an empty submission meets and where its control sits.
    pub(crate) fn controls(&self) -> Vec<ControlCheck> {
        fn mark(nodes: &[Node], inside: bool, in_repeater: &mut [bool], place: &mut [LeafPlace]) {
            for node in nodes {
                match node {
                    Node::Repeater(r) => mark(&r.children.nodes, true, in_repeater, place),
                    Node::Field(_) | Node::Embedded(_) => {
                        node.visit_fields(&mut |index, leaf_place| {
                            in_repeater[index] = inside;
                            place[index] = leaf_place;
                        });
                    }
                    _ => mark(
                        node.children().unwrap_or_default(),
                        inside,
                        in_repeater,
                        place,
                    ),
                }
            }
        }
        let mut in_repeater = vec![false; self.fields.len()];
        let mut place = vec![LeafPlace::Rendered; self.fields.len()];
        mark(&self.nodes, false, &mut in_repeater, &mut place);
        self.fields
            .iter()
            .zip(in_repeater)
            .zip(place)
            .map(|((field, in_repeater), place)| {
                let errors = field.validate("");
                ControlCheck {
                    name: field.name().to_string(),
                    required: !errors.is_empty(),
                    required_error: errors
                        .into_iter()
                        .next()
                        .map(|error| error.message)
                        .or_else(|| Some(required_error(field.label_str()))),
                    in_repeater,
                    place,
                }
            })
            .collect()
    }

    /// Lists keys in `values` that no declared input owns, sorted, so handlers reject
    /// client-controlled writes.
    pub fn unknown_keys(&self, values: &HashMap<String, String>) -> Vec<String> {
        let known: HashSet<&str> = self.fields.iter().map(Field::name).collect();
        let mut out: Vec<String> = values
            .keys()
            .filter(|k| !known.contains(k.as_str()))
            .cloned()
            .collect();
        out.sort();
        out
    }

    /// Reports what is wrong with this declaration: a field whose lens binds no single column, and
    /// two fields sharing a name.
    pub fn declaration_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut seen = HashSet::new();
        for field in &self.fields {
            match field.misdeclared() {
                Some(error) => errors.push(format!("field `{}`: {error}", field.name())),
                None if !seen.insert(field.name()) => errors.push(format!(
                    "duplicate field name '{}': each Schema input needs a distinct field",
                    field.name()
                )),
                None => {}
            }
        }
        errors
    }

    /// Validates submitted values against declared inputs, treating absent keys as empty and
    /// skipping fields in absent repeater groups and hidden variant groups.
    pub fn validate(&self, values: &HashMap<String, String>) -> FieldErrors {
        let mut errors = FieldErrors::new();
        let mut skip: HashSet<String> = HashSet::new();
        walk_absent_groups(
            &self.nodes,
            &self.fields,
            values,
            &mut skip,
            &mut errors,
            false,
        );
        for field in &self.fields {
            if skip.contains(field.name()) {
                continue;
            }
            let value = values.get(field.name()).map(String::as_str).unwrap_or("");
            for error in field.validate(value) {
                errors.push(error);
            }
        }
        errors
    }

    /// Collects field names a submission leaves out of validation.
    pub(crate) fn absent_fields(&self, values: &HashMap<String, String>) -> HashSet<String> {
        let mut skip = HashSet::new();
        let mut discarded = FieldErrors::new();
        walk_absent_groups(
            &self.nodes,
            &self.fields,
            values,
            &mut skip,
            &mut discarded,
            false,
        );
        skip
    }

    /// Reports whether this submission renders an error under `key`, either a visible field or a
    /// repeater label.
    pub(crate) fn renders_error_key(&self, values: &HashMap<String, String>, key: &str) -> bool {
        fn labels(nodes: &[Node], key: &str) -> bool {
            nodes.iter().any(|node| match node {
                Node::Repeater(repeater) => {
                    repeater.label == key || labels(&repeater.children.nodes, key)
                }
                node => node
                    .children()
                    .is_some_and(|children| labels(children, key)),
            })
        }
        let hidden = self.hidden_fields(values);
        let renders_field = self
            .fields
            .iter()
            .any(|field| field.name() == key && !hidden.contains(key));
        renders_field || labels(&self.nodes, key)
    }

    /// Collects field names this submission hides.
    fn hidden_fields(&self, values: &HashMap<String, String>) -> HashSet<String> {
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

    /// Validates submissions and then probes each visible choice's option existence.
    pub async fn validate_async(&self, cx: &Cx, values: &HashMap<String, String>) -> FieldErrors {
        let mut errors = self.validate(values);
        let absent = self.absent_fields(values);
        for field in &self.fields {
            let name = field.name();
            if errors.contains_key(name) || absent.contains(name) {
                continue;
            }
            let Some(value) = values.get(name) else {
                continue;
            };
            for message in field.validate_exists(cx, value).await {
                errors.add(name, message);
            }
        }
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
        let absent = self.absent_fields(values);
        for field in &self.fields {
            let name = field.name();
            if absent.contains(name) {
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
