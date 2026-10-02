//! Embedded values: a typed value and the flat form map converted in one declared place, plus the schema node that renders it.
//!
//! An embedded leaf binds one flattened storage column and a value binds as a whole; a payload selects the variant only when the submission carries no discriminant at all, a shared column never selects one, and an unknown discriminant is refused.
//!
//! # What an app writes
//!
//! ```ignore
//! #[derive(Clone, toasty::Embed, tablo_core::EmbeddedForm)]
//! pub struct Seo { pub title: String, pub description: String }
//!
//! Section::new("SEO").schema(Seo::form(dx, Post::fields().seo()));
//! record.seo.write_form(cx, Post::fields().seo(), &mut values);
//! let seo = Seo::read_form(cx, Post::fields().seo(), &values)?;
//! ```
//!
//! # What is not covered
//!
//! A `#[document]` inside an embedded value, a relation inside one, and an embedded enum nested inside an enum variant are not covered.
//!
//! Every leaf under an embedded step reports nullable.

use std::collections::HashMap;

use tablo_ui::field_group as ui_field_group;
use toasty::stmt::Path;
use topcoat::{Result, context::Cx, view::*};

use super::{
    Schema,
    fields::Field,
    lenses::{DeclCx, FieldResolver},
    tree::{LeafPlace, Mode, Node, Source},
};
use crate::form::{FieldError, FormScalar};

/// Reads an embedded value from and writes it to the flat form map; derive it to generate the value's schema.
pub trait EmbeddedForm: Sized {
    /// Writes this value's leaves into `out`, including the active variant's discriminant for an enum.
    fn write_form<M>(
        &self,
        cx: &Cx,
        parent: impl Into<Path<M, Self>>,
        out: &mut HashMap<String, String>,
    ) where
        M: toasty::schema::Model,
    {
        let schema = Self::build_schema(&DeclCx::from_cx(cx), parent.into());
        self.write_node(schema.embedded_root(), out);
    }

    /// Reads a value back from a submission, taking an embedded enum's variant from the discriminant key and refusing a discriminant that names no variant.
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
        let schema = Self::build_schema(&DeclCx::from_cx(cx), parent.into());
        Self::read_node(schema.embedded_root(), values)
    }

    /// The value's schema: one node holding its resolved fields.
    #[doc(hidden)]
    fn build_schema<M>(dx: &DeclCx, parent: Path<M, Self>) -> Schema
    where
        M: toasty::schema::Model;

    /// Reports whether every leaf answers a blank submission.
    #[doc(hidden)]
    fn answers_blank() -> bool;

    #[doc(hidden)]
    fn write_node(&self, node: &Embedded, out: &mut HashMap<String, String>);

    #[doc(hidden)]
    fn read_node(
        node: &Embedded,
        values: &HashMap<String, String>,
    ) -> std::result::Result<Self, Vec<FieldError>>;
}

/// Holds an embedded value's resolved keys, rendering fields, and variant groups for an enum.
#[doc(hidden)]
#[derive(Debug)]
pub struct Embedded {
    shape: Shape,
}

#[derive(Debug)]
enum Shape {
    Struct(Vec<Member>),
    Enum(EnumNode),
}

#[derive(Debug)]
struct EnumNode {
    key: String,
    discriminant: usize,
    shared: Vec<usize>,
    variants: Vec<Variant>,
}

#[derive(Debug)]
struct Variant {
    value: String,
    members: Vec<Member>,
}

#[derive(Debug)]
enum Member {
    Leaf { key: String, field: Option<usize> },
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

    pub fn key(&self, variant: Option<usize>, index: usize) -> &str {
        match &self.members(variant)[index] {
            Member::Leaf { key, .. } => key,
            Member::Nested(_) => panic!("member {index} is an embedded value, not a leaf"),
        }
    }

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

    pub fn write_variant(&self, index: usize, out: &mut HashMap<String, String>) {
        let e = self.enum_node();
        out.insert(e.key.clone(), e.variants[index].value.clone());
    }

    /// Returns the variant a submission reads as, falling back to the first variant with a submitted payload when it names no discriminant, and refuses an unknown discriminant.
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

    /// Whether a submission mentions any key of this value.
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

    /// Collects every form key the value occupies, each once.
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

    /// Visits every field slot with where the leaf sits in the form.
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

    /// Collects the field slots of every variant group the submission hides, hiding nothing when it names no variant.
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

    /// Renders the value, showing every variant group in a form and only the stored variant's group in a view.
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

/// Renders a struct's or a variant group's members in order.
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

fn is_present(values: &HashMap<String, String>, key: &str) -> bool {
    values
        .get(key)
        .is_some_and(|value| !value.trim().is_empty())
}

/// Builds an embedded value's schema node, one member at a time, in the order the derive declares them.
#[doc(hidden)]
pub struct EmbeddedBuilder {
    fields: Vec<Field>,
    shape: Shape,
    variant: Option<usize>,
}

impl EmbeddedBuilder {
    pub fn structure() -> Self {
        Self {
            fields: Vec::new(),
            shape: Shape::Struct(Vec::new()),
            variant: None,
        }
    }

    /// Builds an enum value at `parent` from the app schema and panics when the schema is missing or `parent` names no embedded enum.
    pub fn enumeration<M, T>(dx: &DeclCx, parent: Path<M, T>) -> Self
    where
        M: toasty::schema::Model,
    {
        let resolver = FieldResolver::new(dx);
        assert!(
            resolver.has_schema(),
            "an embedded enum needs the app schema: build the `DeclCx` from a `Db`"
        );
        let shape = resolver.resolve_enum(parent).unwrap_or_else(|| {
            panic!(
                "{} is not an embedded enum in this app schema",
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

    /// Starts the next variant's members.
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

    pub fn leaf(&mut self, field: impl Into<Field>) {
        let field = field.into();
        let key = field.name().to_string();
        let index = self.fields.len();
        self.fields.push(field);
        self.members_mut().push(Member::Leaf {
            key,
            field: Some(index),
        });
    }

    /// Adds a `#[shared(..)]` leaf that renders once, outside the variant groups.
    pub fn shared(&mut self, field: impl Into<Field>) {
        let field = field.into();
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

    /// Adds a nested value from its own `build_schema`.
    pub fn nested(&mut self, schema: Schema) {
        let Schema { nodes, fields } = schema;
        let Ok([Node::Embedded(mut nested)]) = <[Node; 1]>::try_from(nodes) else {
            panic!("a nested value's schema is its one embedded node");
        };
        nested.offset(self.fields.len());
        self.fields.extend(fields);
        self.members_mut().push(Member::Nested(*nested));
    }

    /// Finishes the value's schema as one embedded node.
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

/// Collects every form key the embedded value at `parent` occupies.
#[doc(hidden)]
pub fn embedded_keys<M, T>(dx: &DeclCx, parent: impl Into<Path<M, T>>) -> Vec<String>
where
    M: toasty::schema::Model,
    T: EmbeddedForm,
{
    T::build_schema(dx, parent.into()).embedded_root().keys()
}

/// Reads one leaf out of a submission by its resolved key, answering a blank with the member's declared blank or the type's own and refusing a blank with neither.
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

/// Moves `result`'s value out, or its errors into `errors`.
#[doc(hidden)]
pub fn take_leaf<T>(
    result: std::result::Result<T, FieldError>,
    errors: &mut Vec<FieldError>,
) -> Option<T> {
    result.map_err(|error| errors.push(error)).ok()
}

#[doc(hidden)]
pub fn take_value<T>(
    result: std::result::Result<T, Vec<FieldError>>,
    errors: &mut Vec<FieldError>,
) -> Option<T> {
    result.map_err(|nested| errors.extend(nested)).ok()
}

#[cfg(test)]
mod tests;
