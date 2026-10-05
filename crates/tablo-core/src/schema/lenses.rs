//! Resolves typed Toasty paths to form keys and field metadata.
//!
//! A path resolves through the app schema in scope: the one the panel installs while it calls a
//! resource's declarations at mount, or the one [`declare`] installs. Outside a scope a path
//! resolves against its model alone, which binds a single field and refuses an embedded leaf.

use std::{cell::RefCell, sync::Arc};

use toasty::stmt::Path;
use toasty_core::stmt::PathRoot;
use topcoat::context::Cx;

thread_local! {
    static SCHEMA: RefCell<Option<Arc<toasty_core::Schema>>> = const { RefCell::new(None) };
}

/// Run `declarations` with `db`'s app schema in scope, so every path they bind resolves an
/// embedded leaf to its flattened storage column.
///
/// Mounting a panel ([`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel)) calls
/// each resource's declarations inside such a scope. Call this to build a declaration outside a
/// panel, such as a [`Schema`](crate::Schema) a custom page renders:
///
/// ```rust,no_run
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
/// # #[derive(Debug, Clone, tablo_core::RecordForm)]
/// # #[form(model = Post)]
/// # struct PostForm { title: String }
/// # use tablo_core::RecordForm;
/// # let db: toasty::Db = todo!();
/// let form = tablo_core::declare(&db, PostForm::schema);
/// # let _ = form;
/// ```
/// ```
pub fn declare<T>(db: &toasty::Db, declarations: impl FnOnce() -> T) -> T {
    declare_with(Some(db.schema().clone()), declarations)
}

/// Run `declarations` with `schema` in scope, restoring the enclosing scope after, a panic
/// included.
pub(crate) fn declare_with<T>(
    schema: Option<Arc<toasty_core::Schema>>,
    declarations: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<Arc<toasty_core::Schema>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SCHEMA.with(|scope| *scope.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(SCHEMA.with(|scope| scope.replace(schema)));
    declarations()
}

/// The app schema of the `Db` a request carries, if any.
pub(crate) fn schema_of(cx: &Cx) -> Option<Arc<toasty_core::Schema>> {
    topcoat::context::try_app_context::<toasty::Db>(cx).map(|db| db.schema().clone())
}

/// A path resolved to the form key it binds and the metadata a field defaults from.
///
/// A column defaults `required` from its nullability and `unique` from a single- or multi-field
/// unique index it belongs to. An embedded leaf is never required or unique by default: only the
/// matching enum variant writes a variant payload column, so the resolver reports every embedded
/// leaf nullable. That is the binding default, not a storage fact — the flattened column of a
/// required embedded struct is `NOT NULL` — so a field opts in with `.required()`.
pub(crate) struct ResolvedLens<M, T> {
    pub(crate) path: Path<M, T>,
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) nullable: bool,
    pub(crate) unique: bool,
    /// Why the path binds no column, when it does not: the builder records this, and
    /// [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel) reports it.
    pub(crate) misdeclared: Option<String>,
}

impl<M, T> ResolvedLens<M, T>
where
    M: toasty::schema::Model,
{
    /// Resolve `path` through the app schema in scope.
    ///
    /// A refused path keeps a placeholder name spelling its steps, so it reads as no other
    /// field's duplicate.
    pub(crate) fn of(path: impl Into<Path<M, T>>) -> Self {
        let path = path.into();
        match FieldResolver::current().resolve(path.clone()) {
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

/// The form key a record-form field binds: the column `path` resolves to.
#[doc(hidden)]
pub fn form_key<M, T>(path: Path<M, T>) -> String
where
    M: toasty::schema::Model,
{
    ResolvedLens::of(path).name
}

/// The storage column a lens resolves to, plus the metadata field
/// constructors need.
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
pub(crate) struct FieldResolver {
    /// The **compiled** schema: its `.app` half resolves the path, its
    /// `.mapping` half names the column, its `.db` half holds the name.
    ///
    /// `Db::schema()` gives all three halves the walk needs: `.app` carries the embedded models
    /// the owned `Model::schema()` cannot see, `.mapping` records which physical column each
    /// field resolves to, and `.db` holds the physical table and column names. So an embedded
    /// lens binds without opening a connection.
    schema: Option<Arc<toasty_core::Schema>>,
}

impl FieldResolver {
    pub(crate) fn new(schema: Option<Arc<toasty_core::Schema>>) -> Self {
        Self { schema }
    }

    /// The resolver over the app schema in scope ([`declare`]).
    pub(crate) fn current() -> Self {
        Self::new(SCHEMA.with(|scope| scope.borrow().clone()))
    }

    /// Whether an app schema is in scope at all.
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
    pub(crate) fn resolve<M, T>(&self, path: Path<M, T>) -> Result<LeafField, String>
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
        if is_embedded_path && let Some(schema) = self.schema.as_deref() {
            return Self::walk_embedded(schema, &core_path).ok_or_else(|| {
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
        schema: &toasty_core::Schema,
        path: &toasty_core::stmt::Path,
    ) -> Option<LeafField> {
        match &path.root {
            PathRoot::Model(id) => {
                let root = schema.app.get_model(*id)?.as_root()?;
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
    pub(crate) fn resolve_enum<M, T>(&self, path: Path<M, T>) -> Option<EnumShape>
    where
        M: toasty::schema::Model,
    {
        let schema = self.schema.as_deref()?;
        let core_path: toasty_core::stmt::Path = path.into();
        let PathRoot::Model(id) = core_path.root else {
            return None;
        };
        let root = schema.app.get_model(id)?.as_root()?;
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
                crate::toasty_compat::value_text(&v.discriminant)
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
/// records.
pub(crate) fn lens_field<M, T>(
    path: Path<M, T>,
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
