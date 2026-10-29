//! Composes field slots, layout containers, and embedded values into one tree.

use std::{
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

use topcoat::{Result, context::Cx, view::*};

use super::{
    Schema,
    condition::{Condition, disable, watch},
    embedded::Embedded,
    fields::{Field, Placement},
    layouts::{Grid, Group, Section, Tab, Tabs},
};
use crate::values::{FieldError, FieldErrors, FormValue, FormValues};

#[derive(Debug)]
pub(crate) enum Node {
    /// A field, by its index in the root schema's field list.
    Field(usize),
    Section(Box<Section>),
    Group(Box<Group>),
    Grid(Box<Grid>),
    /// A `Tabs` block, whose children are its `Tab` nodes.
    Tabs(Box<Tabs>),
    /// One tab, placed only in a `Tabs` block's children.
    Tab(Box<Tab>),
    Embedded(Box<Embedded>),
}

/// Holds the values and errors a schema's controls render with.
pub struct Source<'a> {
    values: &'a FormValues,
    errors: &'a FieldErrors,
    /// The conditions each watched field's key drives.
    watched: HashMap<String, Vec<Condition>>,
    /// The prefix of the form's DOM ids and of its signals' keys; empty for a form alone on its
    /// page.
    scope: String,
    /// Where a searchable choice fetches its options, when not the resource's own route.
    options: Option<String>,
    /// Whether the form edits a stored record.
    editing: bool,
    /// The keys the request's field access hides or locks.
    mask: super::FormMask,
    /// What every control's name starts with: a related form's `{slug}.`, empty otherwise.
    prefix: String,
    /// How many tabs hold the node rendering now, which tells a nested block's DOM ids from its
    /// outer tab's.
    tab_depth: AtomicUsize,
}

impl<'a> Source<'a> {
    /// Renders controls hydrated with `values` and inline `errors`.
    pub fn form(values: &'a FormValues, errors: &'a FieldErrors) -> Self {
        Self {
            values,
            errors,
            watched: HashMap::new(),
            scope: String::new(),
            options: None,
            editing: false,
            mask: super::FormMask::default(),
            prefix: String::new(),
            tab_depth: AtomicUsize::new(0),
        }
    }

    /// Renders the edit form of a stored record, where `disabled_on_edit` disables a control.
    pub(crate) fn editing(mut self, editing: bool) -> Self {
        self.editing = editing;
        self
    }

    /// Renders nothing for the keys `mask` hides, and disables the controls of the ones it locks.
    pub(crate) fn masked(mut self, mask: super::FormMask) -> Self {
        self.mask = mask;
        self
    }

    /// Renders a form sharing its page with another: its DOM ids and its signals' keys carry
    /// `scope`, so neither form's labels, conditions or variants reach the other's.
    pub(crate) fn scoped(mut self, scope: impl Into<String>) -> Self {
        self.scope = scope.into();
        self
    }

    /// Renders a related form inside its owner's: every control posts under `prefix`, while its
    /// values, errors, mask and conditions keep the related form's own keys.
    pub(crate) fn posting_under(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
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
    pub(crate) fn guard<'v>(
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

    /// How many tabs hold the node rendering now.
    pub(crate) fn tab_depth(&self) -> usize {
        self.tab_depth.load(Ordering::Relaxed)
    }

    /// Renders what follows inside `depth` tabs.
    pub(crate) fn set_tab_depth(&self, depth: usize) {
        self.tab_depth.store(depth, Ordering::Relaxed);
    }

    /// Whether `condition` holds in the submission, as the page first renders it: always without
    /// one.
    pub(crate) fn holds(&self, condition: Option<&Condition>) -> bool {
        condition.is_none_or(|condition| condition.holds(self.values))
    }

    pub(crate) fn value(&self, name: &str) -> Option<&FormValue> {
        self.values.get(name)
    }

    /// The text `name` holds: a scalar control's value.
    pub(crate) fn text(&self, name: &str) -> Option<&str> {
        self.values.text(name)
    }

    /// The rows `name` holds: a repeater's, none when it holds none.
    pub(crate) fn rows(&self, name: &str) -> &[FormValues] {
        self.values.rows(name).unwrap_or_default()
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
        self.placement(name, None).id(name)
    }

    /// Where the field posting `name` renders in this form, under its parent's value `parent`.
    fn placement<'p>(&'p self, name: &str, parent: Option<&'p str>) -> Placement<'p> {
        Placement {
            parent,
            scope: &self.scope,
            options: self.options.as_deref(),
            editing: self.editing,
            locked: self.mask.locks(name),
            prefix: &self.prefix,
        }
    }

    /// Returns the message the field `field` renders under its control.
    pub(crate) fn error_for(&self, field: &Field) -> Option<&FieldError> {
        self.errors.first(field.name())
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
                if !shows(&fields[*index], source) {
                    return Ok(().boxed());
                }
                let field = &fields[*index];
                // A repeater's rows render from the keys and errors of each of their controls.
                if let Some(repeater) = field.as_repeater() {
                    let view = Box::pin(repeater.render(cx, field, source)).await?;
                    return Ok(source.guard(cx, field.condition(), view));
                }
                let error = source.error_for(field);
                let parent = field.parent_key().and_then(|key| source.text(key));
                let mut view = Box::pin(field.render_under(
                    cx,
                    source.value(field.name()),
                    error,
                    source.placement(field.name(), parent),
                ))
                .await?;
                if let Some(conditions) = source.watched.get(field.name()) {
                    let posted = format!("{}{}", source.prefix, field.name());
                    view = watch(
                        cx,
                        field,
                        &posted,
                        conditions,
                        &source.scope,
                        source.values,
                        view,
                    );
                }
                if let Some(condition) = field.disabling() {
                    view = disable(cx, condition, &source.scope, source.values, view);
                }
                Ok(source.guard(cx, field.condition(), view))
            }
            // A block whose every field is masked renders no heading over nothing.
            Node::Section(_) | Node::Group(_) | Node::Grid(_) | Node::Tabs(_)
                if !self.shows_any(fields, source) =>
            {
                Ok(().boxed())
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
            Node::Tabs(t) => {
                let view = Box::pin(t.render(cx, fields, source)).await?;
                Ok(source.guard(cx, t.condition.as_ref(), view))
            }
            // A tab renders through its block, which lays out its trigger and its panel.
            Node::Tab(t) => Box::pin(render_nodes(cx, &t.children.nodes, fields, source)).await,
            Node::Embedded(e) => Box::pin(e.render(cx, fields, source)).await,
        }
    }

    /// Whether a field under this node renders an error from `source`: under its own key, or a
    /// repeater's under one of its rows' keys.
    pub(crate) fn has_error(&self, fields: &[Field], source: &Source<'_>) -> bool {
        let mut any = false;
        self.visit_fields(&mut |index| {
            let name = fields[index].name();
            any |= source.errors.iter().any(|error| {
                error.key == name
                    || error
                        .key
                        .strip_prefix(name)
                        .is_some_and(|rest| rest.starts_with('.'))
            });
        });
        any
    }

    /// The first field slot under this node, in declaration order.
    pub(crate) fn first_field(&self) -> Option<usize> {
        let mut first = None;
        self.visit_fields(&mut |index| {
            first.get_or_insert(index);
        });
        first
    }

    /// Calls `f` with every field slot under this node, in declaration order.
    fn visit_fields(&self, f: &mut impl FnMut(usize)) {
        match self {
            Node::Field(index) => f(*index),
            Node::Embedded(e) => e.visit_fields(f),
            node => {
                for child in node.children().unwrap_or_default() {
                    child.visit_fields(f);
                }
            }
        }
    }

    /// Whether some field under this node renders under `source`'s mask.
    pub(crate) fn shows_any(&self, fields: &[Field], source: &Source<'_>) -> bool {
        match self {
            Node::Field(index) => shows(&fields[*index], source),
            Node::Embedded(e) => {
                let mut any = false;
                e.visit_fields(&mut |index| any |= shows(&fields[index], source));
                any
            }
            node => node
                .children()
                .unwrap_or_default()
                .iter()
                .any(|child| child.shows_any(fields, source)),
        }
    }

    /// Returns the nodes a layout container holds, or `None` for a field slot and an embedded
    /// value.
    pub(crate) fn children(&self) -> Option<&[Node]> {
        match self {
            Node::Section(s) => Some(&s.children.nodes),
            Node::Group(g) => Some(&g.children.nodes),
            Node::Grid(g) => Some(&g.children.nodes),
            Node::Tabs(t) => Some(&t.children.nodes),
            Node::Tab(t) => Some(&t.children.nodes),
            Node::Field(_) | Node::Embedded(_) => None,
        }
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match self {
            Node::Section(s) => Some(&mut s.children.nodes),
            Node::Group(g) => Some(&mut g.children.nodes),
            Node::Grid(g) => Some(&mut g.children.nodes),
            Node::Tabs(t) => Some(&mut t.children.nodes),
            Node::Tab(t) => Some(&mut t.children.nodes),
            Node::Field(_) | Node::Embedded(_) => None,
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

impl<F> Schema<F> {
    /// The first field whose control posts keys besides its own or fetches from its resource's
    /// routes: a file field, a repeater, an embedded value's leaf, a searchable or dependent
    /// choice. A related form, posting under its owner's prefix, carries none of them.
    pub(crate) fn needs_own_keys(&self) -> Option<&str> {
        let mut embedded = Vec::new();
        for node in &self.nodes {
            node.embedded_fields(&mut embedded);
        }
        self.fields
            .iter()
            .enumerate()
            .find(|(index, field)| {
                embedded.contains(index)
                    || field.is_file()
                    || field.as_repeater().is_some()
                    || field.parent_key().is_some()
                    || field
                        .as_choice()
                        .is_some_and(|choice| choice.is_searchable())
            })
            .map(|(_, field)| field.name())
    }
}

impl Node {
    /// Adds to `out` every field slot an embedded value under this node holds.
    fn embedded_fields(&self, out: &mut Vec<usize>) {
        match self {
            Node::Embedded(e) => e.visit_fields(&mut |index| out.push(index)),
            node => {
                for child in node.children().unwrap_or_default() {
                    child.embedded_fields(out);
                }
            }
        }
    }
}

/// Whether `field` renders under `source`'s mask: not when hidden, nor a locked repeater, whose
/// rows' order posts its key.
fn shows(field: &Field, source: &Source<'_>) -> bool {
    !(source.mask.hides(field.name())
        || field.is_hidden(source.editing)
        || (source.mask.locks(field.name()) && field.as_repeater().is_some()))
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

impl<F> IntoSchema<F> for Tabs<Schema<F>> {
    fn into_schema(self) -> Schema<F> {
        let mut fields = Vec::new();
        let block = self.map(|mut children| {
            fields = std::mem::take(&mut children.fields);
            children.retype()
        });
        Schema::from_parts(vec![Node::Tabs(Box::new(block))], fields)
    }
}

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
