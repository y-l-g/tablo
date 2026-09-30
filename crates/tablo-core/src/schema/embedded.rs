//! Embedded **values**: a typed value and the flat form map, converted in one
//! declared place, and the schema node that renders it.
//!
//! An embedded leaf binds one column: a lens through an embedded struct, an
//! enum variant, or a `#[document]` resolves to its flattened storage column
//! ([`ResolvedLens::new`](super::ResolvedLens::new)). A **value** binds as a
//! whole. `#[derive(EmbeddedForm)]` builds its schema node — one resolved field
//! per leaf, a nested node per `#[form(embed)]` value, and for an enum the
//! variant control over the discriminant column plus one group per variant —
//! and the codec reads and writes the flat map through that node's keys, so the
//! app never spells a flattened name.
//!
//! A payload selects the variant only when the submission carries no
//! discriminant at all (the create form has nothing to hydrate); a
//! `#[shared(..)]` column belongs to several variants and never selects one. A
//! named discriminant always wins, and an unknown one is refused.
//!
//! # What an app writes
//!
//! ```ignore
//! #[derive(Clone, toasty::Embed, tablo_core::EmbeddedForm)]
//! pub struct Seo { pub title: String, pub description: String }
//!
//! Section::new("SEO").schema(Seo::form(cx, Post::fields().seo()));
//! record.seo.write_form(cx, Post::fields().seo(), &mut values);
//! let seo = Seo::read_form(cx, Post::fields().seo(), &values)?;
//! ```
//!
//! # What is not covered
//!
//! A `#[document]` inside an embedded value (its fields share one column, so no
//! per-field binding; a `#[document]` leaf binds through `ResolvedLens::new`),
//! a relation inside one, and an embedded enum nested inside an enum variant.
//! Nesting inside *structs* works at any depth. With JavaScript off every
//! variant group renders.
//!
//! Every leaf under an embedded step is **not required** by default: only the
//! matching variant writes a variant payload column, so the resolver reports
//! every embedded leaf nullable.

use std::collections::HashMap;

use tablo_ui::field_group as ui_field_group;
use toasty::stmt::Path;
use topcoat::{Result, context::Cx, view::*};

use super::{
    Schema,
    fields::Field,
    lenses::FieldResolver,
    tree::{LeafPlace, Mode, Node, Source},
};
use crate::form::{FieldError, FormScalar};

/// An embedded value that can be read from, and written to, the flat form map.
///
/// Derive it with [`tablo_core::EmbeddedForm`](crate::EmbeddedForm), which also
/// generates `form(cx, parent)`, the value's schema. The hidden methods are
/// the derive's; the two provided ones are the codec.
pub trait EmbeddedForm: Sized {
    /// Write this value's leaves into `out`, under the columns the app schema
    /// resolves for `parent`.
    ///
    /// For an enum, the **active variant's** leaves are written, plus its
    /// discriminant: a value has one variant, and the form must say which.
    fn write_form<M>(
        &self,
        cx: &Cx,
        parent: impl Into<Path<M, Self>>,
        out: &mut HashMap<String, String>,
    ) where
        M: toasty::schema::Model,
    {
        let schema = Self::build_schema(cx, parent.into());
        self.write_node(schema.embedded_root(), out);
    }

    /// Read a value back from a submission.
    ///
    /// An embedded enum takes its variant from the discriminant key; a
    /// submission naming one it does not declare is refused, and one that
    /// carries no discriminant at all falls back to the first variant whose
    /// own payload was submitted, else the first variant.
    ///
    /// # Errors
    ///
    /// Every leaf whose value its type refuses, and a discriminant that names
    /// no variant.
    fn read_form<M>(
        cx: &Cx,
        parent: impl Into<Path<M, Self>>,
        values: &HashMap<String, String>,
    ) -> std::result::Result<Self, Vec<FieldError>>
    where
        M: toasty::schema::Model,
    {
        let schema = Self::build_schema(cx, parent.into());
        Self::read_node(schema.embedded_root(), values)
    }

    /// The value's schema: one node holding its resolved fields.
    #[doc(hidden)]
    fn build_schema<M>(cx: &Cx, parent: Path<M, Self>) -> Schema
    where
        M: toasty::schema::Model;

    /// Whether every leaf answers a blank submission: a declared
    /// `#[form(blank = ..)]`, the scalar's own blank answer, or — for a nested
    /// value — every leaf of that value's.
    ///
    /// The panel's build check reads it: a control a submission can post empty
    /// (an optional one, a variant group's payload, or a control inside a
    /// `Repeater`) whose field answers none is a declaration the panel refuses
    /// rather than a blank the parse would refuse at submit.
    #[doc(hidden)]
    fn answers_blank() -> bool;

    /// Write through a built node.
    #[doc(hidden)]
    fn write_node(&self, node: &Embedded, out: &mut HashMap<String, String>);

    /// Read through a built node.
    #[doc(hidden)]
    fn read_node(
        node: &Embedded,
        values: &HashMap<String, String>,
    ) -> std::result::Result<Self, Vec<FieldError>>;
}

/// An embedded value's schema node: its resolved keys, the fields that render
/// them, and for an enum its variant control and variant groups.
#[doc(hidden)]
#[derive(Debug)]
pub struct Embedded {
    shape: Shape,
}

#[derive(Debug)]
enum Shape {
    /// One member per struct field, in declaration order.
    Struct(Vec<Member>),
    Enum(EnumNode),
}

#[derive(Debug)]
struct EnumNode {
    /// The discriminant column.
    key: String,
    /// The variant control's field slot.
    discriminant: usize,
    /// The `#[shared(..)]` columns' field slots: each renders once, outside
    /// every variant group, because the column belongs to several variants and
    /// must stay editable whichever one is chosen.
    shared: Vec<usize>,
    variants: Vec<Variant>,
}

#[derive(Debug)]
struct Variant {
    /// The discriminant text this variant stores (`2`).
    value: String,
    /// One member per payload field, in declaration order.
    members: Vec<Member>,
}

#[derive(Debug)]
enum Member {
    /// A leaf column: its key, and the field slot that renders it in place —
    /// `None` for a shared column, which renders outside the variant groups.
    Leaf {
        key: String,
        field: Option<usize>,
    },
    Nested(Embedded),
}

impl Embedded {
    fn members(&self, variant: Option<usize>) -> &[Member] {
        match (&self.shape, variant) {
            (Shape::Struct(members), None) => members,
            (Shape::Enum(e), Some(index)) => &e.variants[index].members,
            _ => panic!("a struct member is addressed without a variant, an enum's with one"),
        }
    }

    /// The key of leaf `index` (of `variant`, for an enum).
    pub fn key(&self, variant: Option<usize>, index: usize) -> &str {
        match &self.members(variant)[index] {
            Member::Leaf { key, .. } => key,
            Member::Nested(_) => panic!("member {index} is an embedded value, not a leaf"),
        }
    }

    /// The nested value at member `index` (of `variant`, for an enum).
    pub fn nested(&self, variant: Option<usize>, index: usize) -> &Embedded {
        match &self.members(variant)[index] {
            Member::Nested(nested) => nested,
            Member::Leaf { .. } => panic!("member {index} is a leaf, not an embedded value"),
        }
    }

    fn enum_node(&self) -> &EnumNode {
        match &self.shape {
            Shape::Enum(e) => e,
            Shape::Struct(_) => panic!("a struct value has no variant"),
        }
    }

    /// Write variant `index`'s discriminant into `out`.
    pub fn write_variant(&self, index: usize, out: &mut HashMap<String, String>) {
        let e = self.enum_node();
        out.insert(e.key.clone(), e.variants[index].value.clone());
    }

    /// The variant a submission reads as.
    ///
    /// A discriminant the submission **names** always wins, and one it names
    /// but the enum does not declare is refused rather than read as another
    /// variant. A submission that names none falls back to the first variant,
    /// in declaration order, with a payload of its own submitted — a shared
    /// column belongs to several variants and never selects one — else the
    /// first variant.
    ///
    /// # Errors
    ///
    /// A discriminant that names no variant, keyed by the discriminant column.
    pub fn variant_index(
        &self,
        values: &HashMap<String, String>,
    ) -> std::result::Result<usize, Vec<FieldError>> {
        let e = self.enum_node();
        let submitted = values.get(&e.key).map(|v| v.trim()).unwrap_or_default();
        if submitted.is_empty() {
            let own = |member: &Member| match member {
                Member::Leaf { key, field } => field.is_some() && is_present(values, key),
                Member::Nested(nested) => nested.any_present(values),
            };
            let inferred = e.variants.iter().position(|v| v.members.iter().any(own));
            return Ok(inferred.unwrap_or(0));
        }
        e.variants
            .iter()
            .position(|v| v.value == submitted)
            .ok_or_else(|| {
                vec![FieldError::invalid(
                    e.key.clone(),
                    format!("`{submitted}` is not a valid variant"),
                )]
            })
    }

    /// Whether a submission mentions any key of this value: a leaf's non-empty
    /// value, or an enum's discriminant.
    fn any_present(&self, values: &HashMap<String, String>) -> bool {
        let member = |member: &Member| match member {
            Member::Leaf { key, .. } => is_present(values, key),
            Member::Nested(nested) => nested.any_present(values),
        };
        match &self.shape {
            Shape::Struct(members) => members.iter().any(member),
            Shape::Enum(e) => {
                is_present(values, &e.key)
                    || e.variants.iter().any(|v| v.members.iter().any(member))
            }
        }
    }

    /// Every form key the value occupies, each once: an enum's discriminant
    /// first, then the leaf columns in declaration order.
    pub(crate) fn keys(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect_keys(&mut out);
        out
    }

    fn collect_keys(&self, out: &mut Vec<String>) {
        let push = |members: &[Member], out: &mut Vec<String>| {
            for member in members {
                match member {
                    Member::Leaf { key, .. } if !out.contains(key) => out.push(key.clone()),
                    Member::Leaf { .. } => {}
                    Member::Nested(nested) => nested.collect_keys(out),
                }
            }
        };
        match &self.shape {
            Shape::Struct(members) => push(members, out),
            Shape::Enum(e) => {
                out.push(e.key.clone());
                for variant in &e.variants {
                    push(&variant.members, out);
                }
            }
        }
    }

    /// Shift every field slot by `by` (see [`Node::offset`]).
    pub(crate) fn offset(&mut self, by: usize) {
        let members = |members: &mut Vec<Member>| {
            for member in members {
                match member {
                    Member::Leaf { field, .. } => {
                        if let Some(index) = field {
                            *index += by;
                        }
                    }
                    Member::Nested(nested) => nested.offset(by),
                }
            }
        };
        match &mut self.shape {
            Shape::Struct(list) => members(list),
            Shape::Enum(e) => {
                e.discriminant += by;
                for index in &mut e.shared {
                    *index += by;
                }
                for variant in &mut e.variants {
                    members(&mut variant.members);
                }
            }
        }
    }

    /// Visit every field slot, with where the leaf sits in the form.
    pub(crate) fn visit_fields(&self, place: LeafPlace, f: &mut impl FnMut(usize, LeafPlace)) {
        fn members(members: &[Member], place: LeafPlace, f: &mut impl FnMut(usize, LeafPlace)) {
            for member in members {
                match member {
                    Member::Leaf {
                        field: Some(index), ..
                    } => f(*index, place),
                    Member::Leaf { field: None, .. } => {}
                    Member::Nested(nested) => nested.visit_fields(place, f),
                }
            }
        }
        match &self.shape {
            Shape::Struct(list) => members(list, place, f),
            Shape::Enum(e) => {
                f(e.discriminant, LeafPlace::Discriminant);
                for index in &e.shared {
                    f(*index, place);
                }
                for variant in &e.variants {
                    members(&variant.members, LeafPlace::Payload, f);
                }
            }
        }
    }

    /// The field slots of every variant group `values` hides: a group whose
    /// variant is not the one the submission names. A submission naming no
    /// variant hides nothing, because the payload fallback may read any group.
    pub(crate) fn hidden_fields(&self, values: &HashMap<String, String>, out: &mut Vec<usize>) {
        let nested = |members: &[Member], out: &mut Vec<usize>| {
            for member in members {
                if let Member::Nested(nested) = member {
                    nested.hidden_fields(values, out);
                }
            }
        };
        match &self.shape {
            Shape::Struct(members) => nested(members, out),
            Shape::Enum(e) => {
                let chosen = values.get(&e.key).map(|v| v.trim()).unwrap_or_default();
                for variant in &e.variants {
                    if !chosen.is_empty() && chosen != variant.value {
                        for member in &variant.members {
                            match member {
                                Member::Leaf {
                                    field: Some(index), ..
                                } => out.push(*index),
                                Member::Leaf { field: None, .. } => {}
                                Member::Nested(nested) => nested
                                    .visit_fields(LeafPlace::Payload, &mut |index, _| {
                                        out.push(index)
                                    }),
                            }
                        }
                    } else {
                        nested(&variant.members, out);
                    }
                }
            }
        }
    }

    /// Render the value: a struct's members in order; an enum's variant
    /// control, its shared columns, then its variant groups.
    ///
    /// A form renders every variant group, each marked with `data-variant`
    /// (the value it stores) and `data-variant-of` (the discriminant column),
    /// which `variant.js` reads to keep only the chosen one visible. A view
    /// renders only the stored variant's group and the shared columns it
    /// declares: the rest hold no values.
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        match &self.shape {
            Shape::Struct(members) => render_members(cx, members, fields, source).await,
            Shape::Enum(e) => {
                let mut views = Vec::with_capacity(e.shared.len() + e.variants.len() + 1);
                views.push(
                    Node::Field(e.discriminant)
                        .render(cx, fields, source)
                        .await?,
                );
                let stored = source.value(&e.key).map(str::trim);
                let stored_variant = e
                    .variants
                    .iter()
                    .find(|variant| stored == Some(variant.value.as_str()));
                for index in &e.shared {
                    // A view shows a shared column only when the stored
                    // variant declares it: another variant's column holds no
                    // value on this record.
                    let key = fields[*index].name();
                    let declared = stored_variant.is_some_and(|variant| {
                        variant
                            .members
                            .iter()
                            .any(|member| matches!(member, Member::Leaf { key: k, .. } if k == key))
                    });
                    if source.mode() == Mode::View && !declared {
                        continue;
                    }
                    views.push(Node::Field(*index).render(cx, fields, source).await?);
                }
                for variant in &e.variants {
                    if source.mode() == Mode::View && stored != Some(variant.value.as_str()) {
                        continue;
                    }
                    let members = render_members(cx, &variant.members, fields, source).await?;
                    let value = variant.value.clone();
                    let owner = e.key.clone();
                    views.push(
                        view! {
                            cx =>
                            ui_field_group(
                                attrs: attributes! { data-variant=(value) data-variant-of=(owner) },
                                (members)
                            )
                        }
                        .boxed(),
                    );
                }
                Ok(view! {
                    cx =>
                    for v in views {
                        (v)
                    }
                }
                .boxed())
            }
        }
    }
}

/// The members of a struct or a variant group, in order; a shared leaf renders
/// elsewhere.
async fn render_members<'a>(
    cx: &'a Cx,
    members: &[Member],
    fields: &[Field],
    source: &Source<'_>,
) -> Result<BoxView<'a>> {
    let mut views = Vec::with_capacity(members.len());
    for member in members {
        match member {
            Member::Leaf {
                field: Some(index), ..
            } => views.push(Node::Field(*index).render(cx, fields, source).await?),
            Member::Leaf { field: None, .. } => {}
            Member::Nested(nested) => {
                views.push(Box::pin(nested.render(cx, fields, source)).await?)
            }
        }
    }
    Ok(view! {
        cx =>
        for v in views {
            (v)
        }
    }
    .boxed())
}

/// Whether `values` carries a non-empty value for `key`.
fn is_present(values: &HashMap<String, String>, key: &str) -> bool {
    values
        .get(key)
        .is_some_and(|value| !value.trim().is_empty())
}

/// Builds an embedded value's schema node, one member at a time, in the order
/// the derive declares them.
#[doc(hidden)]
pub struct EmbeddedBuilder {
    fields: Vec<Field>,
    shape: Shape,
    /// The enum variant members are being added to.
    variant: Option<usize>,
}

impl EmbeddedBuilder {
    /// A struct value.
    pub fn structure() -> Self {
        Self {
            fields: Vec::new(),
            shape: Shape::Struct(Vec::new()),
            variant: None,
        }
    }

    /// An enum value at `parent`: its discriminant column and variants come
    /// from the request's app schema, and its variant control is the first
    /// field.
    ///
    /// Panics without a `Db` in context, or when `parent` names no embedded
    /// enum: every caller is a declaration, and a lens that resolves to nothing
    /// is a wiring bug.
    pub fn enumeration<M, T>(cx: &Cx, parent: Path<M, T>) -> Self
    where
        M: toasty::schema::Model,
    {
        let resolver = FieldResolver::from_cx(cx);
        assert!(
            resolver.has_schema(),
            "an embedded enum needs the app schema: put a `Db` in the context (GH #191)"
        );
        let shape = resolver.resolve_enum(parent).unwrap_or_else(|| {
            panic!(
                "{} is not an embedded enum in this request's app schema (GH #191)",
                std::any::type_name::<T>()
            )
        });
        let variants = shape
            .variants
            .iter()
            .map(|(value, _)| Variant {
                value: value.clone(),
                members: Vec::new(),
            })
            .collect();
        Self {
            fields: vec![Field::discriminant(
                shape.discriminant.clone(),
                shape.variants,
            )],
            shape: Shape::Enum(EnumNode {
                key: shape.discriminant,
                discriminant: 0,
                shared: Vec::new(),
                variants,
            }),
            variant: None,
        }
    }

    /// Start the next variant's members.
    pub fn variant(&mut self) {
        let next = self.variant.map_or(0, |index| index + 1);
        let Shape::Enum(e) = &self.shape else {
            panic!("a struct value has no variant");
        };
        assert!(
            next < e.variants.len(),
            "the type declares more variants than the app schema"
        );
        self.variant = Some(next);
    }

    fn members_mut(&mut self) -> &mut Vec<Member> {
        match (&mut self.shape, self.variant) {
            (Shape::Struct(members), _) => members,
            (Shape::Enum(e), Some(index)) => &mut e.variants[index].members,
            (Shape::Enum(_), None) => panic!("an enum member needs `variant()` first"),
        }
    }

    /// A leaf rendered in place.
    pub fn leaf(&mut self, field: Field) {
        let key = field.name().to_string();
        let index = self.fields.len();
        self.fields.push(field);
        self.members_mut().push(Member::Leaf {
            key,
            field: Some(index),
        });
    }

    /// A `#[shared(..)]` leaf: the first variant declaring its column renders
    /// it, once, outside the variant groups.
    pub fn shared(&mut self, field: Field) {
        let key = field.name().to_string();
        let Shape::Enum(e) = &mut self.shape else {
            panic!("a struct value has no shared column");
        };
        if !e
            .shared
            .iter()
            .any(|index| self.fields[*index].name() == key)
        {
            e.shared.push(self.fields.len());
            self.fields.push(field);
        }
        self.members_mut().push(Member::Leaf { key, field: None });
    }

    /// A nested value, from its own `build_schema`.
    pub fn nested(&mut self, schema: Schema) {
        let Schema { nodes, fields } = schema;
        let Ok([Node::Embedded(mut nested)]) = <[Node; 1]>::try_from(nodes) else {
            panic!("a nested value's schema is its one embedded node");
        };
        nested.offset(self.fields.len());
        self.fields.extend(fields);
        self.members_mut().push(Member::Nested(*nested));
    }

    /// The value's schema: one embedded node.
    pub fn finish(self) -> Schema {
        if let Shape::Enum(e) = &self.shape {
            assert_eq!(
                self.variant.map_or(0, |index| index + 1),
                e.variants.len(),
                "the type declares fewer variants than the app schema"
            );
        }
        Schema {
            nodes: vec![Node::Embedded(Box::new(Embedded { shape: self.shape }))],
            fields: self.fields,
        }
    }
}

/// Every form key the embedded value at `parent` occupies: a record form binds
/// them to the one field that holds the value.
#[doc(hidden)]
pub fn embedded_keys<M, T>(cx: &Cx, parent: impl Into<Path<M, T>>) -> Vec<String>
where
    M: toasty::schema::Model,
    T: EmbeddedForm,
{
    T::build_schema(cx, parent.into()).embedded_root().keys()
}

/// Read one leaf out of a submission, by its resolved key.
///
/// Trimmed; an absent or empty value is the member's declared blank answer,
/// else the type's own, and a type with neither is refused inline — the rule
/// ADR-0022 gives a record form's scalar.
///
/// # Errors
///
/// A blank the leaf has no answer for, and a value the type cannot parse,
/// worded as the typed rule words it.
#[doc(hidden)]
pub fn parse_leaf<T>(
    key: &str,
    values: &HashMap<String, String>,
    blank: Option<T>,
) -> std::result::Result<T, FieldError>
where
    T: FormScalar,
{
    let trimmed = values.get(key).map(|raw| raw.trim()).unwrap_or("");
    if trimmed.is_empty() {
        return blank
            .or_else(T::blank)
            .ok_or_else(|| FieldError::required(key));
    }
    T::parse_form(trimmed).map_err(|message| FieldError::invalid(key, message))
}

/// Move `result`'s value out, or its errors into `errors`. Generated
/// `read_node` bodies collect every leaf's error with it.
#[doc(hidden)]
pub fn take_leaf<T>(
    result: std::result::Result<T, FieldError>,
    errors: &mut Vec<FieldError>,
) -> Option<T> {
    result.map_err(|error| errors.push(error)).ok()
}

/// [`take_leaf`] for a nested value's read.
#[doc(hidden)]
pub fn take_value<T>(
    result: std::result::Result<T, Vec<FieldError>>,
    errors: &mut Vec<FieldError>,
) -> Option<T> {
    result.map_err(|nested| errors.extend(nested)).ok()
}

#[cfg(test)]
mod tests;
