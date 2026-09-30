//! Schema tree — `Node`, `Source`, `IntoSchema`, and the tree walks.
//!
//! `Node` composes field slots, layout containers (`layouts`), and embedded
//! values (`embedded`) into one tree. A field slot indexes the root schema's
//! field list, so every walk that needs the fields reads that list, and the
//! walks here only add what the layout says about them: which fields a
//! submission hides, and which container can skip a field's requiredness.

use std::collections::{HashMap, HashSet};

use topcoat::{Result, context::Cx, view::*};

use super::{
    Schema,
    embedded::Embedded,
    fields::Field,
    layouts::{Grid, Group, Repeater, Section},
    validation::required_error,
};

#[derive(Debug)]
pub(crate) enum Node {
    /// A field, by its index in the root schema's field list.
    Field(usize),
    Repeater(Box<Repeater>),
    Section(Box<Section>),
    Group(Box<Group>),
    Grid(Box<Grid>),
    Embedded(Box<Embedded>),
}

/// Where a schema render reads field values and errors from, and which side of
/// the record the render is for.
///
/// [`Source::form`] is the create/edit path; [`Source::view`] is the detail
/// page, where a field shows its stored value instead of a control — a choice
/// its option label, a file field its path — and layout keeps the structure it
/// declares.
pub struct Source<'a> {
    values: &'a HashMap<String, String>,
    errors: Option<&'a HashMap<String, Vec<String>>>,
}

impl<'a> Source<'a> {
    /// A form render: controls hydrated with `values`, with `errors` inline.
    pub fn form(
        values: &'a HashMap<String, String>,
        errors: &'a HashMap<String, Vec<String>>,
    ) -> Self {
        Self {
            values,
            errors: Some(errors),
        }
    }

    /// A read-only render of a record's `values`.
    ///
    /// Every field reads its key from `values`: a key the map lacks renders
    /// `(missing)` and fails a `debug_assert!`.
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

    /// The value for `name`, if this render has one.
    pub(crate) fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// The errors for `name`. A view has none: it renders a stored record, so
    /// a validation slot would describe a submit that cannot happen.
    pub(crate) fn errors_for(&self, name: &str) -> &[String] {
        self.errors
            .and_then(|errors| errors.get(name))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

/// Which reading of a record a render is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Create/edit: fields render as controls, validation applies.
    Form,
    /// Detail page: fields render their stored value, read-only.
    View,
}

impl Node {
    /// Render this node from `source`, reading field slots from `fields`.
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
        }
    }

    /// The nodes a layout container holds; `None` for a field slot and an
    /// embedded value, whose structure is its own.
    pub(crate) fn children(&self) -> Option<&[Node]> {
        match self {
            Node::Repeater(r) => Some(&r.children.nodes),
            Node::Section(s) => Some(&s.children.nodes),
            Node::Group(g) => Some(&g.children.nodes),
            Node::Grid(g) => Some(&g.children.nodes),
            Node::Field(_) | Node::Embedded(_) => None,
        }
    }

    fn children_mut(&mut self) -> Option<&mut [Node]> {
        match self {
            Node::Repeater(r) => Some(&mut r.children.nodes),
            Node::Section(s) => Some(&mut s.children.nodes),
            Node::Group(g) => Some(&mut g.children.nodes),
            Node::Grid(g) => Some(&mut g.children.nodes),
            Node::Field(_) | Node::Embedded(_) => None,
        }
    }

    /// Shift every field slot under this node by `by`: the node joins a
    /// schema whose field list already holds `by` fields.
    pub(crate) fn offset(&mut self, by: usize) {
        match self {
            Node::Field(index) => *index += by,
            Node::Embedded(e) => e.offset(by),
            _ => {
                for child in self.children_mut().unwrap_or_default() {
                    child.offset(by);
                }
            }
        }
    }

    /// Visit every field slot under this node, with whether it sits inside an
    /// embedded enum's variant group.
    pub(crate) fn visit_fields(&self, f: &mut impl FnMut(usize, bool)) {
        match self {
            Node::Field(index) => f(*index, false),
            Node::Embedded(e) => e.visit_fields(false, f),
            _ => {
                for child in self.children().unwrap_or_default() {
                    child.visit_fields(f);
                }
            }
        }
    }
}

/// Render `nodes` in order, as one view.
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

/// Classify the tree's groups against `values`: the repeaters that are absent
/// and the variant groups the submission hides. Adds the names of the fields
/// they hold to `skip`, and a required absent repeater's error to `errors`.
///
/// A repeater whose inner fields are all empty is **absent** — an untouched
/// group submits empty strings or omits the keys, and both count — so its
/// fields' requiredness must not fire, whatever the group's own. A `required`
/// absent repeater records its one label-keyed error, unless it sits inside an
/// already-absent repeater. A repeater with any non-empty inner value is
/// present, and its inner `required` enforces as usual.
///
/// On edit, the untouched-file backfill (GH #90) runs before validation, so a
/// group whose stored file path is non-empty counts as present there even if
/// the browser submitted it empty — a kept file is real group data.
///
/// An embedded enum's variant group is **hidden** when the submission names a
/// different variant: `variant.js` keeps only the named variant's group
/// visible, so a value the user cannot see must not fail the submit.
pub(crate) fn walk_absent_groups(
    nodes: &[Node],
    fields: &[Field],
    values: &HashMap<String, String>,
    skip: &mut HashSet<String>,
    errors: &mut HashMap<String, Vec<String>>,
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
                    if r.required && !inside_absent {
                        errors
                            .entry(r.label.clone())
                            .or_insert_with(|| vec![required_error(&r.label)]);
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

/// Something that becomes a [`Schema`]: a field, a layout block, a tuple of
/// either, or a schema.
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

/// Generate the single-node [`IntoSchema`] impl for every layout container.
///
/// A container's children keep their own field list until the container
/// becomes a node: the list moves up to the schema the node starts, whose
/// field slots it then indexes from 0, so the children's slots stay valid.
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

/// Generate the tuple impls of [`IntoSchema`] from one list per arity.
///
/// One invocation builds the destructured bindings and the appended schemas
/// from the same list, so an element cannot reach one and not the other.
/// Arity eight is the shared ceiling [`IntoColumns`](crate::resource::IntoColumns)
/// documents.
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
