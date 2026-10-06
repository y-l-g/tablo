//! Composes field slots, layout containers, and embedded values into one tree.

use std::collections::{HashMap, HashSet};

use topcoat::{Result, context::Cx, view::*};

use super::{
    Schema,
    embedded::Embedded,
    fields::Field,
    layouts::{Grid, Group, Repeater, Section},
    lenses::FieldResolver,
    validation::required_error,
};
use crate::form::FieldErrors;

#[derive(Debug)]
pub(crate) enum Node {
    /// A field, by its index in the root schema's field list.
    Field(usize),
    Repeater(Box<Repeater>),
    Section(Box<Section>),
    Group(Box<Group>),
    Grid(Box<Grid>),
    Embedded(Box<Embedded>),
    /// An embedded value its schema has not bound yet: [`Schema::bind`] builds it through the app
    /// schema and splices it in place.
    Unbound(Unbound),
}

/// Builds an embedded value's node through the app schema it binds to.
pub(crate) struct Unbound {
    pub(crate) build: Box<dyn Fn(&FieldResolver) -> Schema + Send + Sync>,
    /// The value's type, which the declaration error names.
    pub(crate) value: &'static str,
}

impl std::fmt::Debug for Unbound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Unbound").field(&self.value).finish()
    }
}

/// Reports where a field's control sits in the form.
#[derive(Clone, Copy, Debug)]
pub(crate) enum LeafPlace {
    Rendered,
    Payload,
    Discriminant,
}

/// Holds where a schema render reads field values and errors from for a form or a read-only view.
pub struct Source<'a> {
    values: &'a HashMap<String, String>,
    errors: Option<&'a FieldErrors>,
}

impl<'a> Source<'a> {
    /// Renders controls hydrated with `values` and inline `errors`.
    pub fn form(values: &'a HashMap<String, String>, errors: &'a FieldErrors) -> Self {
        Self {
            values,
            errors: Some(errors),
        }
    }

    /// Renders a record's `values` read-only and renders a missing key as `(missing)`.
    pub fn view(values: &'a HashMap<String, String>) -> Self {
        Self {
            values,
            errors: None,
        }
    }

    pub(crate) fn mode(&self) -> Mode {
        if self.errors.is_some() {
            Mode::Form
        } else {
            Mode::View
        }
    }

    pub(crate) fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// Returns the error `name` renders.
    pub(crate) fn errors_for(&self, name: &str) -> Option<&str> {
        self.errors
            .and_then(|errors| errors.first(name))
            .map(|error| error.message.as_str())
    }
}

/// Distinguishes a form render from a read-only view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Form,
    View,
}

impl Node {
    /// Renders this node from `source`, reading field slots from `fields`.
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        match self {
            Node::Field(index) => {
                let field = &fields[*index];
                Box::pin(field.render(
                    cx,
                    source.value(field.name()),
                    source.errors_for(field.name()),
                    source.mode(),
                ))
                .await
            }
            Node::Repeater(r) => Box::pin(r.render(cx, fields, source)).await,
            Node::Section(s) => Box::pin(s.render(cx, fields, source)).await,
            Node::Group(g) => Box::pin(g.render(cx, fields, source)).await,
            Node::Grid(g) => Box::pin(g.render(cx, fields, source)).await,
            Node::Embedded(e) => Box::pin(e.render(cx, fields, source)).await,
            // `Schema::render` refuses an unbound schema before any node renders.
            Node::Unbound(_) => Ok(().boxed()),
        }
    }

    /// Returns the nodes a layout container holds, or `None` for a field slot and an embedded
    /// value.
    pub(crate) fn children(&self) -> Option<&[Node]> {
        match self {
            Node::Repeater(r) => Some(&r.children.nodes),
            Node::Section(s) => Some(&s.children.nodes),
            Node::Group(g) => Some(&g.children.nodes),
            Node::Grid(g) => Some(&g.children.nodes),
            Node::Field(_) | Node::Embedded(_) | Node::Unbound(_) => None,
        }
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
            Node::Repeater(r) => Some(&mut r.children.nodes),
            Node::Section(s) => Some(&mut s.children.nodes),
            Node::Group(g) => Some(&mut g.children.nodes),
            Node::Grid(g) => Some(&mut g.children.nodes),
            Node::Field(_) | Node::Embedded(_) | Node::Unbound(_) => None,
        }
    }

    /// Shifts every field slot under this node by `by`.
    pub(crate) fn offset(&mut self, by: usize) {
        match self {
            Node::Field(index) => *index += by,
            Node::Embedded(e) => e.offset(by),
            _ => {
                for child in self.children_mut().into_iter().flatten() {
                    child.offset(by);
                }
            }
        }
    }

    /// Visits every field slot under this node with where its control sits in the form.
    pub(crate) fn visit_fields(&self, f: &mut impl FnMut(usize, LeafPlace)) {
        match self {
            Node::Field(index) => f(*index, LeafPlace::Rendered),
            Node::Embedded(e) => e.visit_fields(LeafPlace::Rendered, f),
            _ => {
                for child in self.children().unwrap_or_default() {
                    child.visit_fields(f);
                }
            }
        }
    }
}

/// Builds every unbound node under `nodes` through `resolver` in place, appending its fields to
/// `fields`.
pub(crate) fn bind_nodes(nodes: &mut Vec<Node>, fields: &mut Vec<Field>, resolver: &FieldResolver) {
    let mut bound = Vec::with_capacity(nodes.len());
    for node in std::mem::take(nodes) {
        let mut node = match node {
            Node::Unbound(unbound) => {
                let Schema {
                    nodes: mut built,
                    fields: built_fields,
                } = (unbound.build)(resolver);
                let offset = fields.len();
                for node in &mut built {
                    node.offset(offset);
                }
                fields.extend(built_fields);
                bound.extend(built);
                continue;
            }
            node => node,
        };
        if let Some(children) = node.children_mut() {
            bind_nodes(children, fields, resolver);
        }
        bound.push(node);
    }
    *nodes = bound;
}

/// Collects the value types of the unbound nodes under `nodes`.
pub(crate) fn unbound_values(nodes: &[Node], out: &mut Vec<&'static str>) {
    for node in nodes {
        match node {
            Node::Unbound(unbound) => out.push(unbound.value),
            node => unbound_values(node.children().unwrap_or_default(), out),
        }
    }
}

/// Renders `nodes` in order as one view.
pub(crate) async fn render_nodes<'a>(
    cx: &'a Cx,
    nodes: &[Node],
    fields: &[Field],
    source: &Source<'_>,
) -> Result<BoxView<'a>> {
    let mut views = Vec::with_capacity(nodes.len());
    for node in nodes {
        views.push(node.render(cx, fields, source).await?);
    }
    Ok(view! {
        cx =>
        for v in views {
            (v)
        }
    }
    .boxed())
}

/// Classifies the tree's groups against `values`, skipping fields in all-empty repeater groups and
/// hidden variant groups and reporting a required absent repeater under its label.
pub(crate) fn walk_absent_groups(
    nodes: &[Node],
    fields: &[Field],
    values: &HashMap<String, String>,
    skip: &mut HashSet<String>,
    errors: &mut FieldErrors,
    inside_absent: bool,
) {
    let name = |index: usize| fields[index].name().to_string();
    for node in nodes {
        match node {
            Node::Embedded(e) => {
                let mut hidden = Vec::new();
                e.hidden_fields(values, &mut hidden);
                skip.extend(hidden.into_iter().map(name));
            }
            Node::Repeater(r) => {
                let mut inner = Vec::new();
                node.visit_fields(&mut |index, _| inner.push(name(index)));
                // `all` on an empty list is true: an inputless group is absent.
                let all_empty = inner
                    .iter()
                    .all(|n| values.get(n).map(|v| v.trim().is_empty()).unwrap_or(true));
                if all_empty {
                    skip.extend(inner);
                    if r.required && !inside_absent && !errors.contains_key(&r.label) {
                        errors.add_required(&r.label, required_error(&r.label));
                    }
                }
                walk_absent_groups(
                    &r.children.nodes,
                    fields,
                    values,
                    skip,
                    errors,
                    inside_absent || all_empty,
                );
            }
            _ => {
                if let Some(children) = node.children() {
                    walk_absent_groups(children, fields, values, skip, errors, inside_absent);
                }
            }
        }
    }
}

/// Converts a field, a layout block, a tuple of either, or a schema into a [`Schema`].
pub trait IntoSchema {
    fn into_schema(self) -> Schema;
}

impl IntoSchema for Schema {
    fn into_schema(self) -> Schema {
        self
    }
}

impl IntoSchema for Field {
    fn into_schema(self) -> Schema {
        Schema {
            nodes: vec![Node::Field(0)],
            fields: vec![self],
        }
    }
}

/// Generates the single-node [`IntoSchema`] impl for every layout container.
macro_rules! container_nodes {
    ($($ty:ident),+ $(,)?) => {
        $(
            impl IntoSchema for $ty {
                fn into_schema(mut self) -> Schema {
                    let fields = std::mem::take(&mut self.children.fields);
                    Schema {
                        nodes: vec![Node::$ty(Box::new(self))],
                        fields,
                    }
                }
            }
        )+
    };
}

container_nodes!(Section, Group, Grid, Repeater);

/// Generates the tuple impls of [`IntoSchema`] from one list per arity.
macro_rules! into_schema_tuples {
    ($($T:ident => $v:ident),+ $(,)?) => {
        impl<$($T),+> IntoSchema for ($($T,)+)
        where
            $($T: IntoSchema,)+
        {
            fn into_schema(self) -> Schema {
                let ($($v,)+) = self;
                let mut schema = Schema::empty();
                $( schema.append($v.into_schema()); )+
                schema
            }
        }
    };
}

into_schema_tuples!(A => a, B => b);
into_schema_tuples!(A => a, B => b, C => c);
into_schema_tuples!(A => a, B => b, C => c, D => d);
into_schema_tuples!(A => a, B => b, C => c, D => d, E => e);
into_schema_tuples!(A => a, B => b, C => c, D => d, E => e, F => f);
into_schema_tuples!(A => a, B => b, C => c, D => d, E => e, F => f, G => g);
into_schema_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h
);

#[cfg(test)]
mod tests;
