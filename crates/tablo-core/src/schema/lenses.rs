//! Field lenses — typed Toasty paths and their app-level metadata.
//!
//! Base bridge layer (with `pk`): these two modules are the only schema modules
//! that name `toasty_core` (upstream #114/#183), so upstream churn has one
//! blast radius.
//!
//! [`lens_field`] hands callers the built `app::Field`, so name, label,
//! nullability, storage name, `FieldTy`, `auto` and `constraints` all come from
//! one walk; [`lens_field_unique`] covers uniqueness, which `Field` does not
//! carry because Toasty keeps it on the model's index list.
//!
//! [`FieldResolver`] is the schema-aware walk: with the app schema in hand an
//! embedded path resolves to its **flattened storage column**, so a
//! [`ResolvedLens::new`] binding reaches a field inside an embedded struct or a
//! `#[document]`. Without a schema the single-segment rule applies, because
//! the owned `app::Model` cannot see embedded models.

use std::sync::Arc;

use toasty_core::stmt::PathRoot;
use topcoat::context::Cx;

/// What a declaration may read: the app schema a lens resolves through, and
/// nothing from a request.
///
/// [`Resource::form`](crate::resource::Resource::form) and
/// [`Resource::view`](crate::resource::Resource::view) receive one.
/// [`Panel::build`](crate::panel::Panel::build) builds it from the panel's
/// `Db` and calls each declaration once, so a declaration cannot depend on a
/// user, a tenant or a query string: it has no way to reach them.
///
/// The schema is what binds an embedded leaf
/// ([`ResolvedLens::new`]) to its flattened storage column. A `DeclCx`
/// without one ([`DeclCx::empty`]) resolves single-field lenses only, as a
/// plain [`FieldLens`] does.
#[derive(Clone, Default)]
pub struct DeclCx {
    schema: Option<Arc<toasty_core::Schema>>,
}

impl std::fmt::Debug for DeclCx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeclCx")
            .field("schema", &self.schema.is_some())
            .finish()
    }
}

impl DeclCx {
    /// The declaration context of `db`'s app schema.
    pub fn new(db: &toasty::Db) -> Self {
        Self {
            schema: Some(db.schema().clone()),
        }
    }

    /// A declaration context with no app schema: single-field lenses only.
    pub fn empty() -> Self {
        Self::default()
    }

    /// The declaration context of the `Db` a request carries, or
    /// [`Self::empty`] when it carries none.
    pub fn from_cx(cx: &Cx) -> Self {
        topcoat::context::try_app_context::<toasty::Db>(cx).map_or_else(Self::empty, Self::new)
    }
}

/// Spec alias — ADR-0001 typed lens. Currently uses `toasty::stmt::Path` directly;
/// a richer `FieldLens` trait will replace this alias if Toasty exposes the
/// metadata walk directly (see upstream issue #183).
pub type FieldLens<M, T> = toasty::stmt::Path<M, T>;

/// A field lens resolved to the form key it binds and the metadata a field
/// defaults from.
///
/// Every [`Field`](super::Field) constructor takes one, so a field of any kind
/// binds a top-level column and an embedded one alike:
///
/// ```ignore
/// Field::text(Post::fields().title())                                   // a column
/// Field::text(ResolvedLens::new(cx, Post::fields().seo().title()))      // an embedded leaf
/// ```
///
/// A plain [`FieldLens`] converts through [`From`], resolving against the
/// model alone: that covers a single-field lens and refuses a traversal
/// path, because the owned `app::Model` cannot see embedded models. A refused
/// lens is a misdeclaration the field records and
/// [`Panel::build`](crate::panel::Panel::build) reports.
/// [`ResolvedLens::new`] resolves through the declaration's app schema instead,
/// so an embedded struct field, an enum variant field, or a `#[document]`
/// field arrives as its **flattened storage column** (`seo_title`) — the name
/// the form posts and the record form reads.
///
/// A column defaults `required` from its nullability and `unique` from a
/// single- or multi-field unique index it belongs to. An embedded leaf is
/// never required or unique by default: only the matching enum variant writes
/// a variant payload column, so the resolver reports every embedded leaf
/// nullable. That is the binding default, not a storage fact — the flattened
/// column of a required embedded struct is `NOT NULL` — so a field opts in
/// with `.required()`.
pub struct ResolvedLens<M, T> {
    pub(crate) path: FieldLens<M, T>,
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) nullable: bool,
    pub(crate) unique: bool,
    /// Why the lens binds no column, when it does not: the field built on
    /// it records this, and [`Panel::build`](crate::panel::Panel::build)
    /// reports it.
    pub(crate) misdeclared: Option<String>,
}

impl<M, T> ResolvedLens<M, T>
where
    M: toasty::schema::Model,
{
    /// The form key the lens binds: the column's name, or an embedded leaf's
    /// flattened storage column.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Resolve `path` through the app schema `dx` carries.
    ///
    /// Without one ([`DeclCx::empty`]) this resolves as [`From`] does and
    /// refuses a traversal lens, so a test cannot silently bind the wrong
    /// column.
    pub fn new(dx: &DeclCx, path: FieldLens<M, T>) -> Self {
        let leaf = FieldResolver::new(dx).resolve(path.clone());
        Self::bound(path, leaf)
    }

    /// The lens bound to `leaf`, or a misdeclared placeholder named after
    /// the lens's steps so it reads as no other field's duplicate.
    fn bound(path: FieldLens<M, T>, leaf: Result<LeafField, String>) -> Self {
        match leaf {
            Ok(leaf) => Self {
                path,
                name: leaf.name,
                label: leaf.label,
                nullable: leaf.nullable,
                unique: leaf.unique,
                misdeclared: None,
            },
            Err(error) => {
                let core_path: toasty_core::stmt::Path = path.clone().into();
                Self {
                    path,
                    name: format!("{:?}", core_path.projection.as_slice()),
                    label: String::new(),
                    nullable: true,
                    unique: false,
                    misdeclared: Some(error),
                }
            }
        }
    }
}

impl<M, T> From<FieldLens<M, T>> for ResolvedLens<M, T>
where
    M: toasty::schema::Model,
{
    fn from(path: FieldLens<M, T>) -> Self {
        let model = M::schema();
        let leaf = lens_field(path.clone(), &model).map(|field| LeafField {
            name: field.name.app_unwrap().to_string(),
            label: lens_label(&field),
            nullable: field.nullable(),
            unique: lens_field_unique(&field, model.as_root_unwrap()),
        });
        Self::bound(path, leaf)
    }
}

/// The storage column a lens resolves to, plus the metadata field
/// constructors need.
///
/// Owned because the schema is borrowed from the request context for the length
/// of one call and no borrow of it can be held by a `Schema`.
#[derive(Debug, Clone)]
pub(crate) struct LeafField {
    /// The flattened storage column (`application_seo_title`), or the field's
    /// own name for a single-segment path.
    pub(crate) name: String,
    pub(crate) label: String,
    /// Whether the leaf is nullable. Every leaf the walk resolves reports
    /// `true`: an embedded leaf is never required by default, because only the
    /// matching enum variant writes a variant payload's column. That is this
    /// walk's policy rather than the compiled column's own nullability — the
    /// flattened column of a required embedded struct is `NOT NULL` — so it is
    /// the *binding* default, not a storage fact.
    pub(crate) nullable: bool,
    /// Whether a unique index covers the column. Always `false` for an
    /// embedded leaf, which declares no index of its own.
    pub(crate) unique: bool,
}

/// Walks a lens path against the app schema, resolving embedded steps.
///
/// Kept as a struct rather than free functions so the schema borrow has one
/// owner and the constructors can share one resolution between the name, the
/// label, and the uniqueness check.
pub(crate) struct FieldResolver<'a> {
    /// The **compiled** schema: its `.app` half resolves the path, its
    /// `.mapping` half names the column, its `.db` half holds the name.
    schema: Option<&'a toasty_core::Schema>,
}

impl<'a> FieldResolver<'a> {
    /// Build the resolver from the app schema `dx` carries, if any.
    ///
    /// `Db::schema()` gives all three halves the walk needs: `.app` carries
    /// the embedded models the owned `Model::schema()` cannot see, `.mapping`
    /// records which physical column each field resolves to, and `.db` holds
    /// the physical table and column names. So an embedded lens binds without
    /// opening a connection.
    pub(crate) fn new(dx: &'a DeclCx) -> Self {
        Self {
            schema: dx.schema.as_deref(),
        }
    }

    /// Whether `dx` carries an app schema at all.
    ///
    /// A leaf resolution falls back to the single-segment rule without one; an
    /// embedded enum has no fallback — its discriminant column and variants
    /// come from the schema — so its entry point says so rather than reporting
    /// a traversal-lens error.
    pub(crate) fn has_schema(&self) -> bool {
        self.schema.is_some()
    }

    /// Resolve a lens to its leaf field.
    ///
    /// With a schema this walks the whole projection, so an embedded step lands
    /// on the flattened column. Without one it falls back to the single-segment
    /// rule and refuses a traversal lens, because silently binding the first
    /// segment misbinds in release.
    ///
    /// # Errors
    ///
    /// A lens that resolves to no single column.
    pub(crate) fn resolve<M, T>(&self, path: FieldLens<M, T>) -> Result<LeafField, String>
    where
        M: toasty::schema::Model,
    {
        let model = M::schema();
        let core_path: toasty_core::stmt::Path = path.into();
        let segments = core_path.projection.as_slice().len();
        // A variant root is an embedded-enum payload accessor *whatever* its
        // projection length: the generated accessor rebases onto the variant, so
        // the steps are variant-local and a single-step projection is the
        // payload field. Only a model root with one step is a plain field.
        let is_embedded_path = segments > 1 || matches!(core_path.root, PathRoot::Variant { .. });
        if is_embedded_path && let Some(schema) = self.schema {
            return self
                .walk_embedded(schema, &core_path, &model)
                .ok_or_else(|| {
                    format!(
                        "lens path {projection:?} does not resolve to a single column for {}: \
                         only embedded steps (embedded structs, enum variant fields, and \
                         `#[document]` fields) can be bound, not relation hops, and only when \
                         the model is in the app schema",
                        std::any::type_name::<M>(),
                        projection = core_path.projection.as_slice(),
                    )
                });
        }
        // No schema: the single-segment rule is all an owned `app::Model` can
        // answer, so resolve the first step against `ModelRoot.fields` and let
        // `single_segment` refuse a traversal lens.
        let idx = single_segment(&core_path, "lens")?;
        let field = model
            .as_root_unwrap()
            .fields
            .get(idx)
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "field index {idx} out of bounds for {}",
                    std::any::type_name::<M>()
                )
            });
        Ok(LeafField {
            name: field.name.app_unwrap().to_string(),
            label: lens_label(&field),
            nullable: field.nullable(),
            unique: lens_field_unique(&field, model.as_root_unwrap()),
        })
    }

    /// Walk a lens path to the physical column it names.
    ///
    /// Two sources, each authoritative for one thing:
    ///
    /// - the **app schema** drives the traversal, because it is the only side that knows a field is
    ///   a `#[document]` — a *primitive* whose storage is a model, so its inner fields collapse
    ///   into the one column named after it;
    /// - the **compiled mapping** names the column. `Db::schema().mapping` records, per model,
    ///   which column every field resolves to — flattened embedded structs, enum discriminant and
    ///   payload columns, shared columns, and a document's single column alike — so this reads the
    ///   name off `db::Table` rather than re-deriving Toasty's naming rules.
    ///
    /// Two roots reach here: a **model** root for a plain path (`seo.title`), and
    /// a **variant** root for an enum payload accessor
    /// (`media.video().poster().url()`), where the generated accessor rebases
    /// onto the variant and the projection's steps are variant-local.
    ///
    /// Only embedded steps are followed. A relation hop yields `None`: this
    /// exists for embedded binding, and binding anything else would reintroduce
    /// the misbind the single-segment rule guards against.
    fn walk_embedded(
        &self,
        schema: &'a toasty_core::Schema,
        path: &toasty_core::stmt::Path,
        owner: &toasty::schema::app::Model,
    ) -> Option<LeafField> {
        match &path.root {
            PathRoot::Model(id) => {
                let root = schema.app.get_model(*id)?.as_root()?;
                // The accessor's `ModelId` and the schema's may come from
                // different `models!(..)` expansions, so an id can name a
                // *different* model here. Verify identity by root model name
                // before trusting any index: binding another model's column is
                // the misbind GH #100 exists to prevent.
                if root.name.upper_camel_case() != owner.as_root()?.name.upper_camel_case() {
                    return None;
                }
                let model = schema.mapping.models.get(&root.id)?;
                descend(
                    schema,
                    &root.fields,
                    &model.fields,
                    path.projection.as_slice(),
                )
            }
            // An enum payload: resolve the parent path (it ends at the enum
            // field), then descend into the variant the accessor selected.
            PathRoot::Variant { parent, variant_id } => {
                let root = schema
                    .app
                    .get_model(parent.root.as_model_unwrap())?
                    .as_root()?;
                let model = schema.mapping.models.get(&root.id)?;
                let parent_steps = parent.projection.as_slice();
                // The app side supplies the enum's payload field list, the
                // mapping side its per-variant column mappings.
                let app_field = app_field_at(schema, &root.fields, parent_steps)?;
                let Some(toasty::schema::app::Model::EmbeddedEnum(e)) =
                    app_embedded(schema, app_field)
                else {
                    return None;
                };
                let MappingField::Enum(me) = mapping_field_at(&model.fields, parent_steps)? else {
                    return None;
                };
                let variant = me.variants.get(variant_id.index)?;
                let payloads: Vec<_> = e.variant_fields(variant_id.index).iter().collect();
                descend(
                    schema,
                    &payloads,
                    &variant.fields,
                    path.projection.as_slice(),
                )
            }
        }
    }

    /// The discriminant column and variants of the embedded **enum** at
    /// `path`, or `None` when `path` names anything else.
    ///
    /// [`Self::resolve`] answers "which column does this one leaf occupy"; this
    /// answers "which column carries the variant, and what does each variant
    /// store there", which is what an embedded enum's variant control needs.
    /// Its payload leaves resolve one at a time through [`Self::resolve`].
    ///
    /// `path` addresses the embedded field itself (`Post::fields().publication()`),
    /// not one of its leaves: a variant-rooted path names a *variant* of a
    /// value, not the value, and yields `None`.
    pub(crate) fn resolve_enum<M, T>(&self, path: FieldLens<M, T>) -> Option<EnumShape>
    where
        M: toasty::schema::Model,
    {
        let schema = self.schema?;
        let core_path: toasty_core::stmt::Path = path.into();
        let PathRoot::Model(id) = core_path.root else {
            return None;
        };
        let owner = M::schema();
        let root = schema.app.get_model(id)?.as_root()?;
        // The same identity check `walk_embedded` makes: an accessor's
        // `ModelId` and the schema's can come from different `models!(..)`
        // expansions, so verify the root by name before trusting any index.
        if root.name.upper_camel_case() != owner.as_root()?.name.upper_camel_case() {
            return None;
        }
        let mapping = schema.mapping.models.get(&root.id)?;
        let steps = core_path.projection.as_slice();
        let app_field = app_field_at(schema, &root.fields, steps)?;
        let (toasty::schema::app::Model::EmbeddedEnum(e), MappingField::Enum(me)) = (
            app_embedded(schema, app_field)?,
            mapping_field_at(&mapping.fields, steps)?,
        ) else {
            return None;
        };
        let discriminant =
            column_of(schema, &MappingField::Primitive(me.discriminant.clone()))?.name;
        let variants = e
            .variants
            .iter()
            .map(|v| {
                discriminant_text(&v.discriminant)
                    .map(|value| (value, capitalize(&v.name.snake_case())))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(EnumShape {
            discriminant,
            variants,
        })
    }
}

/// An embedded enum's discriminant column and its variants.
///
/// A variant carries two things, and they are not interchangeable: the
/// **value** its discriminant column stores (`2`, or a string discriminant's
/// own text), which is the form's transport, and the **name** a person reads
/// (`Published`), which is the variant control's label. The name is humanized
/// into sentence case (`In progress`), as a derived field label is; it is a
/// label, never a handle, since the normalization is lossy (`OK` reads `Ok`).
/// Code addresses variants by **declaration index**, the handle the schema
/// itself uses (`VariantId { index }`).
#[derive(Debug, Clone)]
pub(crate) struct EnumShape {
    /// The discriminant column the form carries (`publication`).
    pub(crate) discriminant: String,
    /// Each variant's stored value and name, in declaration order.
    pub(crate) variants: Vec<(String, String)>,
}

/// The text an embedded enum's discriminant stores.
///
/// Toasty's discriminants are integers or strings, and the form carries that
/// same text — so what a submission posts is what a row stores. Anything else
/// is reported rather than guessed.
fn discriminant_text(value: &toasty_core::stmt::Value) -> Option<String> {
    use toasty_core::stmt::Value;
    match value {
        Value::Bool(v) => Some(v.to_string()),
        Value::I8(v) => Some(v.to_string()),
        Value::I16(v) => Some(v.to_string()),
        Value::I32(v) => Some(v.to_string()),
        Value::I64(v) => Some(v.to_string()),
        Value::U8(v) => Some(v.to_string()),
        Value::U16(v) => Some(v.to_string()),
        Value::U32(v) => Some(v.to_string()),
        Value::U64(v) => Some(v.to_string()),
        Value::String(v) => Some(v.clone()),
        _ => None,
    }
}

/// A `mapping::Field`, aliased so the traversal signatures stay readable.
type MappingField = toasty_core::schema::mapping::Field;

/// Walk the path down to the column that stores it.
///
/// `app_fields` and `mapping_fields` are indexed by the same field index, so
/// every step reads both: the app side decides *whether to descend*, the mapping
/// side names *the column*. Neither alone is enough — the mapping cannot say a
/// field is a `#[document]`, and the app schema cannot say which column a leaf
/// occupies without re-deriving Toasty's naming.
fn descend<F>(
    schema: &toasty_core::Schema,
    app_fields: &[F],
    mapping_fields: &[MappingField],
    steps: &[usize],
) -> Option<LeafField>
where
    F: std::borrow::Borrow<toasty::schema::app::Field>,
{
    let (first, rest) = steps.split_first()?;
    let app_field: &toasty::schema::app::Field = app_fields.get(*first)?.borrow();
    let mapping_field = mapping_fields.get(*first)?;
    // A `#[document]`'s inner fields share its one column, so reaching the
    // document is reaching the leaf, however many steps remain.
    if rest.is_empty() || is_document(app_field) {
        return column_of(schema, mapping_field);
    }
    let toasty::schema::app::FieldTy::Embedded(embedded) = &app_field.ty else {
        // A relation hop, or a primitive with steps left over: not an embedded
        // leaf either way.
        return None;
    };
    match schema.app.get_model(embedded.target)? {
        toasty::schema::app::Model::EmbeddedStruct(e) => {
            let MappingField::Struct(ms) = mapping_field else {
                return None;
            };
            descend(schema, &e.fields, &ms.fields, rest)
        }
        toasty::schema::app::Model::EmbeddedEnum(e) => {
            let MappingField::Enum(me) = mapping_field else {
                return None;
            };
            // A variant consumes two steps: the variant index, then the field.
            let (variant_index, tail) = rest.split_first()?;
            let variant = me.variants.get(*variant_index)?;
            let payloads: Vec<_> = e.variant_fields(*variant_index).iter().collect();
            descend(schema, &payloads, &variant.fields, tail)
        }
        _ => None,
    }
}

/// The column a single mapping field occupies, if it is a leaf.
fn column_of(schema: &toasty_core::Schema, field: &MappingField) -> Option<LeafField> {
    let MappingField::Primitive(p) = field else {
        // A path stopping on a struct or enum names no single column, and a
        // relation stores none.
        return None;
    };
    let column = &schema.db.tables[p.column.table.0].columns[p.column.index];
    Some(LeafField {
        name: column.name.clone(),
        label: capitalize(&column.name.replace('_', " ")),
        // Reported nullable by policy, not read off the column: an embedded
        // leaf is never required by default, because only the matching enum
        // variant writes a variant payload's column. The compiled column can be
        // `NOT NULL` — an embedded struct's flattened column is.
        nullable: true,
        unique: false,
    })
}

/// Whether `field` is a `#[document]`: a *primitive* whose storage is a model,
/// so its inner fields collapse into the one column named after it.
fn is_document(field: &toasty::schema::app::Field) -> bool {
    matches!(
        &field.ty,
        toasty::schema::app::FieldTy::Primitive(p)
            if matches!(p.ty, toasty_core::stmt::Type::Model(_))
    )
}

/// The embedded model an app field targets, if it is embedded.
fn app_embedded<'a>(
    schema: &'a toasty_core::Schema,
    field: &toasty::schema::app::Field,
) -> Option<&'a toasty::schema::app::Model> {
    let toasty::schema::app::FieldTy::Embedded(embedded) = &field.ty else {
        return None;
    };
    schema.app.get_model(embedded.target)
}

/// The app-level field `steps` reaches, used by the variant root to find the
/// enum's payload list before descending.
///
/// Recurses through embedded structs: the variant root's parent path can walk
/// through them (an embedded struct holding the enum), not just one step.
fn app_field_at<'a>(
    schema: &'a toasty_core::Schema,
    fields: &'a [toasty::schema::app::Field],
    steps: &[usize],
) -> Option<&'a toasty::schema::app::Field> {
    let (first, rest) = steps.split_first()?;
    let field = fields.get(*first)?;
    if rest.is_empty() {
        return Some(field);
    }
    match app_embedded(schema, field)? {
        toasty::schema::app::Model::EmbeddedStruct(e) => app_field_at(schema, &e.fields, rest),
        // An enum nested in the parent path consumes two steps per level: the
        // variant index, then a variant-local field index. `variant_fields`
        // returns the variant's own slice of the enum's global field list
        // (upstream 7ff180db), so the second step indexes it directly — the
        // generated accessor's index is variant-local, which is the same field
        // either way.
        //
        // The variant step is bounds-checked here first: `variant_fields`
        // indexes `variants[i]` and panics out of range, and this walk answers
        // `None` for a path it cannot resolve (a wrong lens must not abort a
        // request).
        toasty::schema::app::Model::EmbeddedEnum(e) => {
            let (variant, tail) = rest.split_first()?;
            e.variants.get(*variant)?;
            let field = e.variant_fields(*variant).get(*tail.first()?)?;
            if tail.len() == 1 {
                Some(field)
            } else {
                app_field_at(schema, std::slice::from_ref(field), &tail[1..])
            }
        }
        toasty::schema::app::Model::Root(_) => None,
    }
}

/// The mapping field `steps` reaches, used by the variant root to find the
/// enum's per-variant mappings before descending.
fn mapping_field_at<'a>(fields: &'a [MappingField], steps: &[usize]) -> Option<&'a MappingField> {
    let (first, rest) = steps.split_first()?;
    let field = fields.get(*first)?;
    if rest.is_empty() {
        return Some(field);
    }
    match field {
        MappingField::Struct(s) => mapping_field_at(&s.fields, rest),
        MappingField::Enum(e) => {
            let (variant, tail) = rest.split_first()?;
            mapping_field_at(&e.variants.get(*variant)?.fields, tail)
        }
        _ => None,
    }
}

/// Resolve a typed lens to the app-level [`Field`] behind it.
///
/// The single walk over `Path → toasty_core::stmt::Path → projection`, reading
/// off the model's built schema (upstream issues #114/#183). Callers read
/// name, label, nullability, storage name, `FieldTy`, `auto`, and `constraints`
/// off the result; uniqueness is the one property a `Field` cannot answer, so
/// it has [`lens_field_unique`] of its own.
///
/// `model` is the owning `Model::schema()`, passed in so one form-input
/// construction resolves the schema once (never per row) and shares it with
/// [`lens_field_unique`]. The returned `Field` is owned because
/// `Model::schema()` builds the model by value with no cache, so no borrow of
/// it can escape — but only the one field is cloned.
///
/// Traversal lenses are refused: a multi-step path has no single
/// field name, and silently binding its first segment misbinds in release.
/// Use [`FieldResolver`] to bind an embedded path instead.
///
/// # Errors
///
/// A traversal lens, worded as the misdeclaration the declaring builder
/// records ([`LensBinding`]).
pub(crate) fn lens_field<M, T>(
    path: FieldLens<M, T>,
    model: &toasty::schema::app::Model,
) -> Result<toasty::schema::app::Field, String>
where
    M: toasty::schema::Model,
{
    let core_path: toasty_core::stmt::Path = path.into();
    let idx = single_segment(&core_path, "lens")?;
    Ok(model
        .as_root_unwrap()
        .fields
        .get(idx)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "field index {idx} out of bounds for {}",
                std::any::type_name::<M>()
            )
        }))
}

/// The name and label a column, a filter or a relation binds through a
/// single-field lens, or the misdeclaration that refuses it.
///
/// A builder records the error rather than panicking, and
/// [`Panel::build`](crate::panel::Panel::build) reports it. The placeholder
/// name spells the lens, so two refused lenses are not also reported as a
/// duplicate name.
pub(crate) struct LensBinding {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) misdeclared: Option<String>,
}

impl LensBinding {
    pub(crate) fn of<M, T>(path: FieldLens<M, T>) -> Self
    where
        M: toasty::schema::Model,
    {
        let core_path: toasty_core::stmt::Path = path.clone().into();
        match lens_field(path, &M::schema()) {
            Ok(field) => Self {
                name: field.name.app_unwrap().to_string(),
                label: lens_label(&field),
                misdeclared: None,
            },
            Err(error) => Self {
                name: format!("{:?}", core_path.projection.as_slice()),
                label: String::new(),
                misdeclared: Some(error),
            },
        }
    }
}

/// The capitalized label for a field's app-level name.
pub(crate) fn lens_label(field: &toasty::schema::app::Field) -> String {
    capitalize(field.name.app_unwrap())
}

/// Whether `field` is backed by a unique index it participates in.
///
/// Uniqueness is not a property of a `Field`: Toasty stores it on the model's
/// index list, so this needs the owning `ModelRoot` too. The primary key is
/// excluded — a key column is unique by construction, not by a declared
/// constraint.
///
/// A **composite** unique index counts too: `#[unique(tenant_id, email)]` is how
/// a tenant-scoped resource expresses "unique within the tenant", and the
/// app-side pre-check has to recognise it or the field's `unique()` declaration
/// is silently dead. Recognizing the index is not checking it exactly: the
/// pre-check probes inside the tenant-scoped query's scope, so it enforces the
/// constraint only when that scope matches the index's remaining components.
/// An index with components outside the scope stays a gap, and it is not
/// checkable here because a query's filters are not introspectable; see
/// `Resource::query`'s note and upstream #117.
///
/// This reports declared schema uniqueness, not a global guarantee: SQL permits
/// multiple `NULL`s in a unique index, and enum-variant columns are
/// storage-nullable, so a nullable unique field can still repeat.
pub(crate) fn lens_field_unique(
    field: &toasty::schema::app::Field,
    model: &toasty::schema::app::ModelRoot,
) -> bool {
    model.indices.iter().any(|index| {
        index.unique && !index.primary_key && index.fields.iter().any(|f| f.field == field.id)
    })
}

/// The one field a lens path addresses.
///
/// A traversal lens (relation hops, embedded steps) has no single field name,
/// nullability, or uniqueness — silently binding its first segment misbinds in
/// release, so every lens helper refuses a multi-segment path instead.
///
/// # Errors
///
/// A path of any other length than one, naming `what` bound it.
pub(crate) fn single_segment(path: &toasty_core::stmt::Path, what: &str) -> Result<usize, String> {
    match path.projection.as_slice() {
        [index] => Ok(*index),
        steps => Err(format!(
            "{what} requires a single-field lens, got a {}-segment traversal path",
            steps.len()
        )),
    }
}

/// A field's human label from its storage name.
///
/// Sentence case, with the underscores a Rust column name carries read as
/// spaces: `word_count` is "Word count", not "Word_count". A label is the one
/// place a column name becomes prose, so it should not leak the identifier
/// (`Media_poster_url`, #192). An explicit
/// [`.label(..)`](crate::schema::TextField::label) still wins.
pub(crate) fn capitalize(s: &str) -> String {
    let spaced = s.replace('_', " ");
    let mut c = spaced.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[cfg(test)]
mod tests;
