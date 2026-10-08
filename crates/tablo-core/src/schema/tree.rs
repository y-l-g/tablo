//! Composes field slots, layout containers, and embedded values into one tree.

use std::collections::HashMap;

use topcoat::{Result, context::Cx, view::*};

use super::{
    Schema,
    condition::{Condition, watch},
    embedded::Embedded,
    fields::{Field, Placement},
    layouts::{Grid, Group, Section},
    lenses::FieldResolver,
};
use crate::form::FieldErrors;

#[derive(Debug)]
pub(crate) enum Node {
    /// A field, by its index in the root schema's field list.
    Field(usize),
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

/// Holds the values and errors a schema's controls render with.
pub struct Source<'a> {
    values: &'a HashMap<String, String>,
    errors: &'a FieldErrors,
    /// The conditions each watched field's key drives.
    watched: HashMap<String, Vec<Condition>>,
    /// The prefix of the form's DOM ids and of its signals' keys; empty for a form alone on its
    /// page.
    scope: String,
    /// Where a searchable choice fetches its options, when not the resource's own route.
    options: Option<String>,
}

impl<'a> Source<'a> {
    /// Renders controls hydrated with `values` and inline `errors`.
    pub fn form(values: &'a HashMap<String, String>, errors: &'a FieldErrors) -> Self {
        Self {
            values,
            errors,
            watched: HashMap::new(),
            scope: String::new(),
            options: None,
        }
    }

    /// Renders a form sharing its page with another: its DOM ids and its signals' keys carry
    /// `scope`, so neither form's labels, conditions or variants reach the other's.
    pub(crate) fn scoped(mut self, scope: impl Into<String>) -> Self {
        self.scope = scope.into();
        self
    }

    /// Fetches a searchable choice's options from `url` rather than the resource's own route.
    pub(crate) fn options_at(mut self, url: impl Into<String>) -> Self {
        self.options = Some(url.into());
        self
    }

    /// The prefix of the form's DOM ids and of its signals' keys.
    pub(crate) fn scope(&self) -> &str {
        &self.scope
    }

    /// Renders each watched field with the handlers its conditions follow.
    pub(crate) fn watching(mut self, watched: HashMap<String, Vec<Condition>>) -> Self {
        self.watched = watched;
        self
    }

    /// Renders `view` guarded by `condition`, or as it is without one.
    fn guard<'v>(
        &self,
        cx: &'v Cx,
        condition: Option<&Condition>,
        view: BoxView<'v>,
    ) -> BoxView<'v> {
        match condition {
            Some(condition) => condition.guard(cx, &self.scope, self.values, view),
            None => view,
        }
    }

    pub(crate) fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// The errors the controls render under.
    pub(crate) fn errors(&self) -> &FieldErrors {
        self.errors
    }

    /// Where a searchable choice fetches its options, when not the resource's own route.
    pub(crate) fn options(&self) -> Option<&str> {
        self.options.as_deref()
    }

    /// The DOM id of the control posting `name`.
    pub(crate) fn id(&self, name: &str) -> String {
        self.placement(None).id(name)
    }

    /// Where a field renders in this form, under its parent's value `parent`.
    fn placement<'p>(&'p self, parent: Option<&'p str>) -> Placement<'p> {
        Placement {
            parent,
            scope: &self.scope,
            options: self.options.as_deref(),
        }
    }

    /// Returns the message the field `field` renders under its control.
    pub(crate) fn error_for(&self, field: &Field) -> Option<String> {
        self.errors
            .first(field.name())
            .map(|error| error.message(field.label_str()))
    }
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
                // A repeater's rows render from the keys and errors of each of their controls.
                if let Some(repeater) = field.as_repeater() {
                    let view = Box::pin(repeater.render(cx, field, source)).await?;
                    return Ok(source.guard(cx, field.condition(), view));
                }
                let error = source.error_for(field);
                let parent = field.parent_key().and_then(|key| source.value(key));
                let mut view = Box::pin(field.render_under(
                    cx,
                    source.value(field.name()),
                    error.as_deref(),
                    source.placement(parent),
                ))
                .await?;
                if let Some(conditions) = source.watched.get(field.name()) {
                    view = watch(cx, field, conditions, &source.scope, source.values, view);
                }
                Ok(source.guard(cx, field.condition(), view))
            }
            Node::Section(s) => {
                let view = Box::pin(s.render(cx, fields, source)).await?;
                Ok(source.guard(cx, s.condition.as_ref(), view))
            }
            Node::Group(g) => {
                let view = Box::pin(g.render(cx, fields, source)).await?;
                Ok(source.guard(cx, g.condition.as_ref(), view))
            }
            Node::Grid(g) => {
                let view = Box::pin(g.render(cx, fields, source)).await?;
                Ok(source.guard(cx, g.condition.as_ref(), view))
            }
            Node::Embedded(e) => Box::pin(e.render(cx, fields, source)).await,
            // `Schema::render` refuses an unbound schema before any node renders.
            Node::Unbound(_) => Ok(().boxed()),
        }
    }

    /// Returns the nodes a layout container holds, or `None` for a field slot and an embedded
    /// value.
    pub(crate) fn children(&self) -> Option<&[Node]> {
        match self {
            Node::Section(s) => Some(&s.children.nodes),
            Node::Group(g) => Some(&g.children.nodes),
            Node::Grid(g) => Some(&g.children.nodes),
            Node::Field(_) | Node::Embedded(_) | Node::Unbound(_) => None,
        }
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
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
                    ..
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

/// Converts a field, a layout block, a tuple of either, or a schema into a [`Schema`].
///
/// `F` is the record form whose controls the schema places, `()` for a page's or an action's
/// schema: a control converts into the schema of its own form only.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be placed in a `Schema<{F}>`",
    label = "not a control, layout block, schema, or tuple of 2 to 12 of them for this schema",
    note = "a resource's form places its record form's own controls, from `controls()`; \
            `Field::text` and the other constructors build a page's or an action's controls"
)]
pub trait IntoSchema<F = ()> {
    fn into_schema(self) -> Schema<F>;
}

/// Moves a control or a schema to the form `G`: what `#[derive(RecordForm)]` hands its controls
/// over with.
#[doc(hidden)]
pub trait Retype {
    type As<G>;

    fn retype<G>(self) -> Self::As<G>;
}

impl<F> IntoSchema<F> for Schema<F> {
    fn into_schema(self) -> Schema<F> {
        self
    }
}

impl IntoSchema for Field {
    fn into_schema(self) -> Schema {
        Schema::from_parts(vec![Node::Field(0)], vec![self])
    }
}

/// Generates the single-node [`IntoSchema`] impls for every layout container, holding a schema or
/// nothing.
macro_rules! container_nodes {
    ($($ty:ident),+ $(,)?) => {
        $(
            impl<F> IntoSchema<F> for $ty<Schema<F>> {
                fn into_schema(self) -> Schema<F> {
                    let mut fields = Vec::new();
                    let block = self.map(|mut children| {
                        fields = std::mem::take(&mut children.fields);
                        children.retype()
                    });
                    Schema::from_parts(vec![Node::$ty(Box::new(block))], fields)
                }
            }

            impl<F> IntoSchema<F> for $ty<()> {
                fn into_schema(self) -> Schema<F> {
                    self.holding(Schema::<F>::default()).into_schema()
                }
            }
        )+
    };
}

container_nodes!(Section, Group, Grid);

/// Generates the tuple impls of [`IntoSchema`] from one list per arity.
macro_rules! into_schema_tuples {
    ($($T:ident => $v:ident),+ $(,)?) => {
        impl<Form, $($T),+> IntoSchema<Form> for ($($T,)+)
        where
            $($T: IntoSchema<Form>,)+
        {
            fn into_schema(self) -> Schema<Form> {
                let ($($v,)+) = self;
                let mut schema = Schema::default();
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
into_schema_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i
);
into_schema_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j
);
into_schema_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j, K => k
);
into_schema_tuples!(
    A => a, B => b, C => c, D => d, E => e, F => f, G => g, H => h, I => i, J => j, K => k, L => l
);
