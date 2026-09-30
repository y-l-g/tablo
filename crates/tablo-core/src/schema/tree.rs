//! Schema tree — `Node`, `IntoSchema`, and the tree walks.
//!
//! `Node` composes field leaves (`fields`) and containers (`layouts`)
//! into one tree; `for_each_field` is the single traversal the facade
//! collectors share, and `walk_absent_groups` classifies the groups a
//! submission leaves out.

use std::collections::{HashMap, HashSet};

use topcoat::{Result, context::Cx, view::*};

use super::{
    Schema,
    fields::{FileUpload, Select, TextInput, Textarea},
    layouts::{Grid, Group, Repeater, Section, Tabs},
    validation::required_error,
};

#[derive(Debug)]
pub(crate) enum Node {
    TextInput(Box<TextInput>),
    Textarea(Box<Textarea>),
    Select(Box<Select>),
    FileUpload(Box<FileUpload>),
    Repeater(Box<Repeater>),
    Tabs(Box<Tabs>),
    Section(Box<Section>),
    Group(Box<Group>),
    Grid(Box<Grid>),
}

impl Node {
    /// Render this node from `source` (internal; see [`Schema::render_with`]).
    pub(crate) async fn render_source<'a>(
        &self,
        cx: &'a Cx,
        source: &RenderSource<'_>,
    ) -> Result<BoxView<'a>> {
        match self {
            Node::TextInput(f) => {
                let val = source.value(f.field_name());
                let errs = source.errors_for(f.field_name());
                Ok(Box::pin(f.render_with(cx, val, errs, source.mode))
                    .await?
                    .boxed())
            }
            Node::Textarea(f) => {
                let val = source.value(f.field_name());
                let errs = source.errors_for(f.field_name());
                Ok(Box::pin(f.render_with(cx, val, errs, source.mode))
                    .await?
                    .boxed())
            }
            Node::Select(f) => {
                let val = source.value(f.field_name());
                let errs = source.errors_for(f.field_name());
                Ok(Box::pin(f.render_with(cx, val, errs, source.mode))
                    .await?
                    .boxed())
            }
            Node::FileUpload(f) => {
                let val = source.value(f.field_name());
                let errs = source.errors_for(f.field_name());
                Ok(Box::pin(f.render_with(cx, val, errs, source.mode))
                    .await?
                    .boxed())
            }
            Node::Repeater(r) => Ok(Box::pin(r.render_source(cx, source)).await?.boxed()),
            Node::Tabs(t) => Ok(Box::pin(t.render_source(cx, source)).await?.boxed()),
            Node::Section(s) => Ok(Box::pin(s.render_source(cx, source)).await?.boxed()),
            Node::Group(g) => Ok(Box::pin(g.render_source(cx, source)).await?.boxed()),
            Node::Grid(g) => Ok(Box::pin(g.render_source(cx, source)).await?.boxed()),
        }
    }
}

/// Where a schema render reads field values and errors from (GH #154 §4), and
/// which side of the record the render is for.
///
/// Plain maps, no bindings: `Mode::Form` is the create/edit path, and
/// `Mode::View` renders the detail page, where a field shows its stored value
/// instead of a control — `Select` its option label, `FileUpload` its path —
/// and layout keeps the structure it declares. A struct with one source shape
/// (`Mode::Form` or `Mode::View`) and no second case to name.
pub(crate) struct RenderSource<'a> {
    pub(crate) values: &'a HashMap<String, String>,
    pub(crate) errors: &'a HashMap<String, Vec<String>>,
    pub(crate) mode: Mode,
}

/// Which reading of a record a render is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Create/edit: fields render as controls, validation applies.
    Form,
    /// Detail page: fields render their stored value, read-only.
    View,
}

impl RenderSource<'_> {
    /// The submitted value for `name`, if this render has one.
    pub(crate) fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// The errors for `name`, if this render has any.
    ///
    /// View mode never has any: the detail page renders a stored
    /// record, so a validation slot would describe a submit that cannot happen.
    /// The one place that rule lives, so a layout reading errors cannot forget
    /// it.
    pub(crate) fn errors_for(&self, name: &str) -> &[String] {
        if self.mode == Mode::View {
            return &[];
        }
        self.errors.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// The submitted value for `name`; an absent key validates as `""`.
fn value_of<'a>(values: &'a HashMap<String, String>, name: &str) -> &'a str {
    values.get(name).map(String::as_str).unwrap_or("")
}

/// Validate the field leaf in `node`: the field name and the errors
/// for its submitted value, or `None` when `node` is a container or repeater.
///
/// The one match over field kinds in the validation path — a new kind is one
/// arm here, not another accessor copy and another loop.
pub(crate) fn validate_leaf<'a>(
    node: &'a Node,
    values: &HashMap<String, String>,
) -> Option<(&'a str, Vec<String>)> {
    match node {
        Node::TextInput(f) => Some((f.field_name(), f.validate(value_of(values, f.field_name())))),
        Node::Textarea(f) => Some((f.field_name(), f.validate(value_of(values, f.field_name())))),
        Node::Select(f) => Some((f.field_name(), f.validate(value_of(values, f.field_name())))),
        Node::FileUpload(f) => Some((f.field_name(), f.validate(value_of(values, f.field_name())))),
        _ => None,
    }
}

impl Node {
    /// Nested schema for container nodes; `None` for leaf fields.
    pub(crate) fn children(&self) -> Option<&Schema> {
        match self {
            Node::Repeater(r) => r.children.as_ref(),
            Node::Section(s) => s.children.as_ref(),
            Node::Group(g) => g.children.as_ref(),
            Node::Grid(g) => g.children.as_ref(),
            Node::Tabs(t) => t.children.as_ref(),
            Node::TextInput(_) | Node::Textarea(_) | Node::Select(_) | Node::FileUpload(_) => None,
        }
    }
}

/// The one field walk: visit `node`, then every node nested in containers.
///
/// Container recursion lives here (with `Node::children`), so `Schema::leaves`
/// (and so the `*_inputs` collectors), `Schema::field_names` and
/// `Schema::validate` share one traversal: a new field variant adds an arm at
/// the leaf matches ([`validate_leaf`], the accessors' `pick`), and a new
/// container variant touches only `children` plus this walk.
pub(crate) fn for_each_field(node: &Node, f: &mut impl FnMut(&Node)) {
    f(node);
    if let Some(children) = node.children() {
        for nested in &children.nodes {
            for_each_field(nested, f);
        }
    }
}

/// Classify the schema's groups against `values`: the
/// repeaters that are absent and the variant groups the submission hides.
///
/// For every Repeater, all its inner field names (as `field_names()` of the
/// child schema) are checked: an all-empty group is "absent" — an untouched
/// group submits empty strings or omits the keys, and both count as absent —
/// so its inner field names go into `skip` (their per-field `required` must
/// not fire, whatever the group's own requiredness) and its subtree is
/// walked as absent, suppressing nested required repeaters. A `required`
/// all-empty group also records its one label-keyed error, unless it sits
/// inside an already-absent ancestor. A group with any non-empty inner value
/// counts as present: nothing is skipped, nothing suppressed, and inner
/// `required` enforces as usual.
///
/// On edit, the GH #90 untouched-file backfill runs before validation, so a
/// group whose stored file path is non-empty counts as present there even if
/// the browser submitted it empty — a kept file is real group data.
///
/// A `Group` marked as one embedded enum variant's payload is
/// **hidden** when the submission names a different variant
/// ([`Group::hidden`](super::layouts::Group::hidden)): `variant.js` keeps only
/// the named variant's group visible, so a value the user cannot see must not
/// fail the submit. A hidden group's whole subtree joins `skip` and its
/// required repeaters are suppressed with it, exactly as an absent one.
pub(crate) fn walk_absent_groups(
    nodes: &[Node],
    values: &HashMap<String, String>,
    skip: &mut HashSet<String>,
    errors: &mut HashMap<String, Vec<String>>,
    inside_absent: bool,
) {
    for node in nodes {
        if let Node::Group(g) = node
            && g.hidden(values)
        {
            if let Some(child) = node.children() {
                skip.extend(child.field_names());
                walk_absent_groups(&child.nodes, values, skip, errors, true);
            }
            continue;
        }
        if let Node::Repeater(r) = node {
            let inner_names = r
                .children
                .as_ref()
                .map(|s| s.field_names())
                .unwrap_or_default();
            // `all` on an empty list is true: an inputless group is absent.
            let all_empty = inner_names
                .iter()
                .all(|n| values.get(n).map(|v| v.trim().is_empty()).unwrap_or(true));
            let absent = inside_absent || all_empty;
            if all_empty {
                skip.extend(inner_names);
                if r.required && !inside_absent {
                    errors
                        .entry(r.label.clone())
                        .or_insert_with(|| vec![required_error(&r.label)]);
                }
            }
            if let Some(child) = node.children() {
                walk_absent_groups(&child.nodes, values, skip, errors, absent);
            }
            continue;
        }
        if let Some(child) = node.children() {
            walk_absent_groups(&child.nodes, values, skip, errors, inside_absent);
        }
    }
}

pub trait IntoSchema {
    fn into_schema(self) -> Schema;
}

impl IntoSchema for Schema {
    fn into_schema(self) -> Schema {
        self
    }
}
/// Generate the [`Node`] conversion and the single-node [`IntoSchema`] impl for
/// every schema type.
///
/// Each type shares its name with its `Node` variant, so one list drives both
/// impls: a new schema type is one entry here, not two hand-written blocks.
macro_rules! schema_nodes {
    ($($ty:ident),+ $(,)?) => {
        $(
            impl From<$ty> for Node {
                fn from(v: $ty) -> Self {
                    Node::$ty(Box::new(v))
                }
            }

            impl IntoSchema for $ty {
                fn into_schema(self) -> Schema {
                    Schema {
                        nodes: vec![self.into()],
                    }
                }
            }
        )+
    };
}

schema_nodes!(
    TextInput, Textarea, Section, Group, Grid, Select, FileUpload, Repeater, Tabs
);

/// Generate the tuple impls of [`IntoSchema`] from one list per arity.
///
/// One invocation builds the destructured bindings and the `nodes` vector from
/// the same list, so an element cannot reach one and not the other. Arity eight
/// is the shared ceiling [`IntoColumns`](crate::resource::IntoColumns)
/// documents.
macro_rules! into_schema_tuples {
    ($($T:ident => $v:ident),+ $(,)?) => {
        impl<$($T),+> IntoSchema for ($($T,)+)
        where
            $($T: Into<Node>,)+
        {
            fn into_schema(self) -> Schema {
                let ($($v,)+) = self;
                Schema {
                    nodes: vec![$($v.into(),)+],
                }
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
