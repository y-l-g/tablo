//! Embedded values: a typed value and its [`FormValues`], one key per leaf, converted in one
//! declared place, plus the schema node that renders it.
//!
//! An embedded leaf binds one flattened storage column and a value binds as a whole; a payload
//! selects the variant only when the submission carries no discriminant at all, a shared column
//! never selects one, and an unknown discriminant is refused.
//!
//! # What an app writes
//!
//! ```rust
//! # use tablo_core::{EmbeddedForm, FieldError, FormValues, Section};
//! #[derive(Debug, Clone, toasty::Embed, tablo_core::EmbeddedForm)]
//! pub struct Seo {
//!     pub title: String,
//!     pub description: String,
//! }
//! # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
//! # struct Post { #[key] #[auto] id: uuid::Uuid, seo: Seo }
//! # fn round_trip(record: &Post) -> Result<(), Vec<FieldError>> {
//! # let mut values = FormValues::new();
//!
//! Section::new("SEO").schema(Seo::form(Post::fields().seo()));
//! record.seo.write_form(Post::fields().seo(), &mut values);
//! let seo = Seo::read_form(Post::fields().seo(), &values)?;
//! # let _ = seo;
//! # Ok(())
//! # }
//! ```
//!
//! # What is not covered
//!
//! A `#[document]` inside an embedded value, a relation inside one, and an embedded enum nested
//! inside an enum variant are not covered.
//!
//! Every leaf under an embedded step reports nullable.

use tablo_ui::field_group as ui_field_group;
use toasty::stmt::Path;
use topcoat::{
    Result,
    context::Cx,
    runtime::{Event, signal},
    view::*,
};

use super::{
    Schema,
    condition::Condition,
    fields::Field,
    lenses::{self, Binding},
    tree::{Node, Source},
};
use crate::{
    form::{FieldError, FormField, FormValues},
    naming::capitalize,
    topcoat_compat::async_page,
};

/// Reads an embedded value from and writes it to [`FormValues`], one key per leaf; derive it to
/// generate the value's schema.
pub trait EmbeddedForm: Sized {
    /// Writes this value's leaves into `out`, including the active variant's discriminant for an
    /// enum.
    fn write_form<M>(&self, parent: impl Into<Path<M, Self>>, out: &mut FormValues)
    where
        M: toasty::schema::Model,
    {
        let schema = Self::build_schema(parent.into());
        self.write_node(schema.embedded_root(), out);
    }

    /// Reads a value back from a submission, taking an embedded enum's variant from the
    /// discriminant key and refusing a discriminant that names no variant.
    ///
    /// # Errors
    ///
    /// Every leaf whose value its type refuses, and a discriminant that names
    /// no variant.
    fn read_form<M>(
        parent: impl Into<Path<M, Self>>,
        values: &FormValues,
    ) -> std::result::Result<Self, Vec<FieldError>>
    where
        M: toasty::schema::Model,
    {
        let schema = Self::build_schema(parent.into());
        Self::read_node(schema.embedded_root(), values)
    }

    /// The value's schema: one node holding its fields, each bound to the column it names.
    #[doc(hidden)]
    fn build_schema<M>(parent: Path<M, Self>) -> Schema
    where
        M: toasty::schema::Model;

    #[doc(hidden)]
    fn write_node(&self, node: &Embedded, out: &mut FormValues);

    #[doc(hidden)]
    fn read_node(
        node: &Embedded,
        values: &FormValues,
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

    pub fn write_variant(&self, index: usize, out: &mut FormValues) {
        let e = self.enum_node();
        out.insert(e.key.clone(), e.variants[index].value.as_str());
    }

    /// Returns the variant a submission reads as, falling back to the first variant with a
    /// submitted payload when it names no discriminant, and refuses an unknown discriminant.
    pub fn variant_index(
        &self,
        values: &FormValues,
    ) -> std::result::Result<usize, Vec<FieldError>> {
        let e = self.enum_node();
        let submitted = values.text(&e.key).map(str::trim).unwrap_or_default();
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
    fn any_present(&self, values: &FormValues) -> bool {
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

    /// Visits every field slot the value renders.
    pub(crate) fn visit_fields(&self, f: &mut impl FnMut(usize)) {
        fn members(members: &[Member], f: &mut impl FnMut(usize)) {
            for member in members {
                match member {
                    Member::Leaf {
                        field: Some(index), ..
                    } => f(*index),
                    Member::Leaf { field: None, .. } => {}
                    Member::Nested(nested) => nested.visit_fields(f),
                }
            }
        }
        match &self.shape {
            Shape::Struct(list) => members(list, f),
            Shape::Enum(e) => {
                f(e.discriminant);
                for index in &e.shared {
                    f(*index);
                }
                for variant in &e.variants {
                    members(&variant.members, f);
                }
            }
        }
    }

    /// Collects the field slots of every variant group the submission hides, hiding nothing when it
    /// names no variant.
    pub(crate) fn hidden_fields(&self, values: &FormValues, out: &mut Vec<usize>) {
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
                let chosen = values.text(&e.key).map(str::trim).unwrap_or_default();
                for variant in &e.variants {
                    if !chosen.is_empty() && chosen != variant.value {
                        for member in &variant.members {
                            match member {
                                Member::Leaf {
                                    field: Some(index), ..
                                } => out.push(*index),
                                Member::Leaf { field: None, .. } => {}
                                Member::Nested(nested) => {
                                    nested.visit_fields(&mut |index| out.push(index))
                                }
                            }
                        }
                    } else {
                        nested(&variant.members, out);
                    }
                }
            }
        }
    }

    /// Renders the value's controls: an enum's every variant group, showing the chosen variant's.
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        match &self.shape {
            Shape::Struct(members) => render_members(cx, members, fields, source).await,
            Shape::Enum(e) => {
                let discriminant = Node::Field(e.discriminant)
                    .render(cx, fields, source)
                    .await?;
                let stored = source.text(&e.key).map(str::trim);
                let mut shared = Vec::with_capacity(e.shared.len());
                for index in &e.shared {
                    shared.push(Node::Field(*index).render(cx, fields, source).await?);
                }
                let mut groups = Vec::with_capacity(e.variants.len());
                for variant in &e.variants {
                    let members = render_members(cx, &variant.members, fields, source).await?;
                    groups.push((variant.value.clone(), members));
                }
                let key = e.key.clone();
                let stored = stored.unwrap_or_default().to_string();
                // The variant the select names is a signal, so choosing another shows its group
                // in place. The other groups sit in a disabled fieldset: a hidden required control
                // would still fail the browser's validation and block the submit, and the server
                // parses only the chosen variant.
                // Keyed by the page's path: navigation carries the values of signals two pages
                // share, and another record's form starts from its own stored variant.
                // Keyed by the form's scope too, as a condition's signal is.
                let page = topcoat::context::try_request_context::<http::request::Parts>(cx)
                    .map(|parts| parts.uri.path().to_string())
                    .unwrap_or_default();
                let scope = source.scope().to_string();
                Ok(async_page(async move {
                    let variant = signal(
                        &cx.keyed(("tablo-variant", page, scope, key.as_str())),
                        move || stored,
                    );
                    let chosen = variant.clone();
                    let groups: Vec<BoxView<'a>> = groups
                        .into_iter()
                        .map(|(value, members)| {
                            let (shown, enabled) = (variant.clone(), variant.clone());
                            let named = value.clone();
                            view! {
                                cx =>
                                <fieldset
                                    class="contents"
                                    :disabled=$(enabled.get() != named)
                                >
                                    ui_field_group(
                                        attrs: attributes! {
                                            data-variant=(value.clone())
                                            :hidden=$(shown.get() != value)
                                        },
                                        (members)
                                    )
                                </fieldset>
                            }
                            .boxed()
                        })
                        .collect();
                    Ok(view! {
                        cx =>
                        <div
                            class="contents"
                            @change=$(|e: Event| chosen.set(e.target.value))
                        >
                            (discriminant)
                        </div>
                        for v in shared {
                            (v)
                        }
                        for g in groups {
                            (g)
                        }
                    })
                }))
            }
        }
    }
}

impl Embedded {
    /// Renders the value `values` spells read-only: each [shown](Self::shown) field's label over
    /// its stored value.
    pub(crate) fn display<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        values: &FormValues,
    ) -> BoxView<'a> {
        let views: Vec<BoxView<'a>> = self
            .shown(fields, values)
            .into_iter()
            .map(|index| {
                let field = &fields[index];
                field.display(cx, values.text(field.name()).unwrap_or_default())
            })
            .collect();
        view! {
            cx =>
            for v in views {
                (v)
            }
        }
        .boxed()
    }

    /// The slots of the fields a reader of the value `values` spells sees, in order: a struct's
    /// leaves, or an enum's discriminant, the shared leaves its stored variant declares, and that
    /// variant's own leaves. An enum whose stored discriminant names no variant shows nothing.
    pub(crate) fn shown(&self, fields: &[Field], values: &FormValues) -> Vec<usize> {
        let mut out = Vec::new();
        self.shown_into(fields, values, &mut out);
        out
    }

    fn shown_into(&self, fields: &[Field], values: &FormValues, out: &mut Vec<usize>) {
        let members = |members: &[Member], out: &mut Vec<usize>| {
            for member in members {
                match member {
                    Member::Leaf {
                        field: Some(index), ..
                    } => out.push(*index),
                    Member::Leaf { field: None, .. } => {}
                    Member::Nested(nested) => nested.shown_into(fields, values, out),
                }
            }
        };
        match &self.shape {
            Shape::Struct(list) => members(list, out),
            Shape::Enum(e) => {
                let stored = values.text(&e.key).map(str::trim);
                let Some(variant) = e
                    .variants
                    .iter()
                    .find(|variant| stored == Some(variant.value.as_str()))
                else {
                    return;
                };
                out.push(e.discriminant);
                // A shared column belongs to the variants that declare it; a unit variant stores
                // nothing in it.
                out.extend(e.shared.iter().copied().filter(|index| {
                    let key = fields[*index].name();
                    variant
                        .members
                        .iter()
                        .any(|member| matches!(member, Member::Leaf { key: k, .. } if k == key))
                }));
                members(&variant.members, out);
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

fn is_present(values: &FormValues, key: &str) -> bool {
    values
        .text(key)
        .is_some_and(|value| !value.trim().is_empty())
}

/// Builds an embedded value's schema node, one member at a time, in the order the derive declares
/// them.
#[doc(hidden)]
pub struct EmbeddedBuilder {
    fields: Vec<Field>,
    shape: Shape,
    variant: Option<usize>,
    /// Whether the enum's model compiles to no schema, so the type, not the schema, lists its
    /// variants.
    refused: bool,
}

impl EmbeddedBuilder {
    pub fn structure() -> Self {
        Self {
            fields: Vec::new(),
            shape: Shape::Struct(Vec::new()),
            variant: None,
            refused: false,
        }
    }

    /// Builds an enum value at `parent` and panics when `parent` names no embedded enum.
    ///
    /// On a model no driver compiles, the variant control carries the error to mount instead, and
    /// each `variant()` opens one as the type declares it.
    pub fn enumeration<M, T>(parent: &Path<M, T>) -> Self
    where
        M: toasty::schema::Model,
    {
        let (binding, variants, refused) = match lenses::resolve_enum(parent) {
            Ok(Some(shape)) => {
                let label = capitalize(&shape.discriminant);
                (
                    Binding::named(shape.discriminant, label),
                    shape.variants,
                    false,
                )
            }
            Ok(None) => panic!(
                "{} is not an embedded enum of {}",
                std::any::type_name::<T>(),
                std::any::type_name::<M>()
            ),
            Err(error) => (Binding::refused(parent, error), Vec::new(), true),
        };
        let discriminant = Field::discriminant(binding, variants.clone());
        Self {
            shape: Shape::Enum(EnumNode {
                key: discriminant.name().to_string(),
                discriminant: 0,
                shared: Vec::new(),
                variants: variants
                    .into_iter()
                    .map(|(value, _)| Variant {
                        value,
                        members: Vec::new(),
                    })
                    .collect(),
            }),
            fields: vec![discriminant],
            variant: None,
            refused,
        }
    }

    /// Starts the next variant's members.
    pub fn variant(&mut self) {
        let next = self.variant.map_or(0, |index| index + 1);
        let Shape::Enum(e) = &mut self.shape else {
            panic!("a struct value has no variant");
        };
        if self.refused {
            e.variants.push(Variant {
                value: next.to_string(),
                members: Vec::new(),
            });
        }
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

    /// Adds a leaf, required when it has no blank answer.
    pub fn leaf(&mut self, field: impl Into<Field>, required: bool) {
        let mut field = field.into();
        field.set_required(required);
        let key = field.name().to_string();
        let index = self.fields.len();
        self.fields.push(field);
        self.members_mut().push(Member::Leaf {
            key,
            field: Some(index),
        });
    }

    /// Adds a `#[shared(..)]` leaf that renders once, outside the variant groups, and shows only
    /// while the discriminant names a variant that declares it.
    pub fn shared(&mut self, field: impl Into<Field>, required: bool) {
        let mut field = field.into();
        field.set_required(required);
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
        let Schema { nodes, fields, .. } = schema;
        let Ok([Node::Embedded(mut nested)]) = <[Node; 1]>::try_from(nodes) else {
            panic!("a nested value's schema is its one embedded node");
        };
        nested.offset(self.fields.len());
        self.fields.extend(fields);
        self.members_mut().push(Member::Nested(*nested));
    }

    /// Finishes the value's schema as one embedded node.
    pub fn finish(mut self) -> Schema {
        if let Shape::Enum(e) = &self.shape {
            assert_eq!(
                self.variant.map_or(0, |index| index + 1),
                e.variants.len(),
                "the type declares fewer variants than the app schema"
            );
            // A shared column every variant declares shows for every one of them. One a variant
            // does not declare shows only while the discriminant names a variant that does, so it
            // neither renders nor submits there. A submission naming no variant reads the variant
            // its payload names, so it keeps the column too.
            for index in &e.shared {
                let key = self.fields[*index].name().to_string();
                let declares = |variant: &Variant| {
                    variant
                        .members
                        .iter()
                        .any(|member| matches!(member, Member::Leaf { key: k, .. } if *k == key))
                };
                let declaring: Vec<String> = e
                    .variants
                    .iter()
                    .filter(|variant| declares(variant))
                    .map(|variant| variant.value.clone())
                    .collect();
                if declaring.len() < e.variants.len() {
                    assert!(
                        self.fields[*index].condition().is_none(),
                        "a shared column's visibility comes from the variants that declare it"
                    );
                    let values = std::iter::once(String::new()).chain(declaring);
                    self.fields[*index].set_condition(Condition::watching(e.key.clone(), values));
                }
            }
        }
        Schema::from_parts(
            vec![Node::Embedded(Box::new(Embedded { shape: self.shape }))],
            self.fields,
        )
    }
}

/// The record-form field binding the embedded value at `parent`: every form key it occupies, and
/// the ones its leaves require.
#[doc(hidden)]
pub fn embedded_field<M, T, K>(
    parent: impl Into<Path<M, T>>,
    field: K,
    name: &'static str,
) -> FormField<K>
where
    M: toasty::schema::Model,
    T: EmbeddedForm,
{
    let schema = T::build_schema(parent.into());
    FormField {
        field,
        name,
        keys: schema.embedded_root().keys(),
        required: schema
            .fields()
            .filter(|field| field.is_required())
            .map(|field| field.name().to_string())
            .collect(),
    }
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
