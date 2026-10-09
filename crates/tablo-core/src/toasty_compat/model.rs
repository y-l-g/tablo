//! What Tablo reads off a model's schema: its fields, their columns, its indices and relations
//! (upstream #183).
//!
//! Toasty exposes a model's schema only as its internal `app`, `mapping` and `db` structures. This
//! module is the one reader of them, and answers in Tablo's own terms: [`FieldInfo`],
//! [`LeafField`], [`EnumShape`], [`BelongsTo`].

use std::sync::Arc;

use toasty::{
    schema::{app, mapping},
    stmt::{IntoExpr, IntoInsert, Path},
};
use toasty_core::stmt::PathRoot;
use topcoat::context::Cx;

use crate::{DeclarationErrorKind, naming::capitalize};

/// A model's id in the app schema.
pub(crate) type ModelId = app::ModelId;

/// The compiled schema of the app's `Db`: the only place an embedded value's columns are known.
///
/// `Db::schema()` gives all three halves the walk needs: `.app` carries the embedded models the
/// owned `Model::schema()` cannot see, `.mapping` records which physical column each field
/// resolves to, and `.db` holds the physical table and column names. So an embedded lens binds
/// without opening a connection.
#[derive(Clone)]
pub(crate) struct AppSchema(Arc<toasty_core::Schema>);

impl AppSchema {
    /// `db`'s compiled schema.
    pub(crate) fn of_db(db: &toasty::Db) -> Self {
        Self(db.schema().clone())
    }

    /// The compiled schema of the `Db` the context carries, if it carries one.
    pub(crate) fn of(cx: &Cx) -> Option<Self> {
        topcoat::context::try_app_context::<toasty::Db>(cx).map(Self::of_db)
    }

    /// The compiled schema itself, for the walks of [`super::join`].
    pub(crate) fn inner(&self) -> &toasty_core::Schema {
        &self.0
    }

    /// Whether the schema registers a model named `name`, in upper camel case.
    pub(crate) fn registers(&self, name: &str) -> bool {
        self.0
            .app
            .models()
            .any(|model| model.name().upper_camel_case() == name)
    }
}

/// A typed path with its model and value types erased: the steps a declaration resolves.
#[derive(Debug, Clone)]
pub(crate) struct ModelPath(toasty_core::stmt::Path);

impl ModelPath {
    pub(crate) fn of<M, T>(path: &Path<M, T>) -> Self {
        Self(path.clone().into())
    }

    /// The field index of each step, from the path's root.
    pub(crate) fn steps(&self) -> &[usize] {
        self.0.projection.as_slice()
    }

    /// Whether the path steps into an embedded value: more than one step, or an enum variant's
    /// root.
    ///
    /// A variant root is an embedded-enum payload accessor *whatever* its projection length: the
    /// generated accessor rebases onto the variant, so the steps are variant-local and a
    /// single-step projection is the payload field. Only a model root with one step is a plain
    /// field.
    pub(crate) fn is_embedded(&self) -> bool {
        self.steps().len() > 1 || matches!(self.0.root, PathRoot::Variant { .. })
    }

    /// `<path> = value`, with the value typed as the path's own field.
    pub(crate) fn eq<T: IntoExpr<T>>(&self, value: T) -> toasty::stmt::Expr<bool> {
        let rhs: toasty_core::stmt::Expr = value.into_expr().into();
        toasty::stmt::Expr::from_untyped(toasty_core::stmt::Expr::eq(
            self.0.clone().into_stmt(),
            rhs,
        ))
    }
}

/// One named field of a model's root, as a declaration check reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldInfo {
    /// The field's index in its model, which the create builder and the insert use too.
    pub(crate) index: usize,
    /// The field's name, which is also its form key.
    pub(crate) name: String,
    /// The label a declaration defaults to: the name, capitalized.
    pub(crate) label: String,
    pub(crate) nullable: bool,
    /// Whether a unique index covers the field.
    ///
    /// A **composite** unique index counts too: `#[unique(tenant_id, email)]` is how a
    /// tenant-scoped resource expresses "unique within the tenant", and the app-side pre-check
    /// has to recognise it or the field's `unique()` declaration is silently dead. Recognizing the
    /// index is not checking it exactly: the pre-check probes inside the tenant-scoped query's
    /// scope, so it enforces the constraint only when that scope matches the index's remaining
    /// components. An index with components outside the scope stays a gap, and it is not
    /// checkable here because a query's filters are not introspectable; see `Resource::query`'s
    /// note and upstream #117.
    ///
    /// The primary key is excluded: a key column is unique by construction, not by a declared
    /// constraint. This reports declared schema uniqueness, not a global guarantee: SQL permits
    /// multiple `NULL`s in a unique index, so a nullable unique field can still repeat.
    pub(crate) unique: bool,
    /// Whether the field is a relation, which stores no column of its own.
    pub(crate) relation: bool,
}

/// Every named field of `M`'s root, in declaration order.
pub(crate) fn fields<M: toasty::schema::Model>() -> Vec<FieldInfo> {
    let model = M::schema();
    let root = model.as_root_unwrap();
    root.fields
        .iter()
        .filter_map(|field| field_info(field, root))
        .collect()
}

/// Whether `M`'s field `name` is a `#[has_many(via = ..)]`, which stores no column and reads
/// through other relations.
pub(crate) fn is_via<M: toasty::schema::Model>(name: &str) -> bool {
    M::schema().as_root().is_some_and(|root| {
        root.fields.iter().any(|field| {
            field.name.app.as_deref() == Some(name) && matches!(field.ty, app::FieldTy::Via(_))
        })
    })
}

/// The one field of `M` a single-step `path` names.
///
/// # Errors
///
/// A traversal path: it has no single field name, and silently binding its first segment
/// misbinds in release.
pub(crate) fn field<M: toasty::schema::Model>(
    path: &ModelPath,
) -> Result<FieldInfo, DeclarationErrorKind> {
    let model = M::schema();
    let root = model.as_root_unwrap();
    let index = single_segment(path)?;
    let field = root.fields.get(index).unwrap_or_else(|| {
        panic!(
            "field index {index} out of bounds for {}",
            std::any::type_name::<M>()
        )
    });
    Ok(field_info(field, root).expect("a model root's fields are named"))
}

fn field_info(field: &app::Field, root: &app::ModelRoot) -> Option<FieldInfo> {
    let name = field.name.app.as_deref()?;
    Some(FieldInfo {
        index: field.id.index,
        name: name.to_string(),
        label: capitalize(name),
        nullable: field.nullable(),
        unique: root.indices.iter().any(|index| {
            index.unique && !index.primary_key && index.fields.iter().any(|f| f.field == field.id)
        }),
        relation: field.is_relation(),
    })
}

/// The one step a path takes.
///
/// # Errors
///
/// A path of any other length than one.
pub(crate) fn single_segment(path: &ModelPath) -> Result<usize, DeclarationErrorKind> {
    match path.steps() {
        [index] => Ok(*index),
        steps => Err(DeclarationErrorKind::TraversalLens { steps: steps.len() }),
    }
}

/// A `belongs_to` relation of a model: its name, the model it targets, and the foreign-key
/// columns it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BelongsTo {
    pub(crate) name: String,
    pub(crate) target: ModelId,
    pub(crate) foreign_keys: Vec<String>,
}

/// The `belongs_to` relation at field `index` of `M`; `None` when that field is no `belongs_to`
/// or names no foreign key.
pub(crate) fn belongs_to<M: toasty::schema::Model>(index: usize) -> Option<BelongsTo> {
    let model = M::schema();
    let root = model.as_root()?;
    let field = root.fields.get(index)?;
    let app::FieldTy::BelongsTo(relation) = &field.ty else {
        return None;
    };
    let foreign_keys = relation
        .foreign_key
        .fields
        .iter()
        .filter_map(|key| root.fields.get(key.source.index))
        .map(|field| field.name.app_unwrap().to_string())
        .collect::<Vec<_>>();
    (!foreign_keys.is_empty()).then(|| BelongsTo {
        name: field.name.app_unwrap().to_string(),
        target: relation.target,
        foreign_keys,
    })
}

/// Which of `M`'s root fields the create builder fills before any setter runs: `#[auto]` fields,
/// which the database fills, and `#[default(..)]` ones, by field index.
///
/// Read off `M::Create::default()`, because toasty keeps a `#[default]` in its generated code
/// only, never in the app schema.
pub(crate) fn prefilled_fields<M: toasty::schema::Model>() -> Vec<bool> {
    let insert = <M::Create as Default>::default().into_insert();
    let toasty_core::stmt::Expr::Stmt(statement) = toasty_core::stmt::Expr::from(insert) else {
        return Vec::new();
    };
    statement
        .stmt
        .as_insert()
        .and_then(|insert| insert.source.body.as_values())
        .and_then(|values| values.rows.last())
        .and_then(|row| row.as_record())
        .map(|record| {
            record
                .fields
                .iter()
                .map(|expr| !expr.is_value_null())
                .collect()
        })
        .unwrap_or_default()
}

/// The storage column a lens resolves to, plus the metadata field constructors need.
#[derive(Debug, Clone)]
pub(crate) struct LeafField {
    /// The flattened storage column (`application_seo_title`), or the field's own name for a
    /// single-segment path.
    pub(crate) name: String,
    pub(crate) label: String,
    /// Whether the leaf is nullable. Every leaf the embedded walk resolves reports `true`,
    /// because only the matching enum variant writes a variant payload's column. That is this
    /// walk's policy rather than the compiled column's own nullability — the flattened column of
    /// a required embedded struct is `NOT NULL` — so it is the *binding* default, not a storage
    /// fact.
    pub(crate) nullable: bool,
    /// Whether a unique index covers the column. Always `false` for an embedded leaf, which
    /// declares no index of its own.
    pub(crate) unique: bool,
}

impl From<FieldInfo> for LeafField {
    fn from(field: FieldInfo) -> Self {
        Self {
            name: field.name,
            label: field.label,
            nullable: field.nullable,
            unique: field.unique,
        }
    }
}

/// Resolve a single-field path against its own model, which needs no app schema.
///
/// # Errors
///
/// A traversal path.
pub(crate) fn single_field<M: toasty::schema::Model>(
    path: &ModelPath,
) -> Result<LeafField, DeclarationErrorKind> {
    field::<M>(path).map(LeafField::from)
}

/// Resolve an embedded path of the model named `model` through `schema`; without one, the path is
/// a traversal the single-field rule refuses, since binding its first segment misbinds in release.
///
/// # Errors
///
/// No schema to resolve through, or a path that reaches no single column.
pub(crate) fn embedded_leaf(
    schema: Option<&AppSchema>,
    path: &ModelPath,
    model: &'static str,
) -> Result<LeafField, DeclarationErrorKind> {
    let steps = path.steps();
    let Some(AppSchema(schema)) = schema else {
        return Err(DeclarationErrorKind::TraversalLens { steps: steps.len() });
    };
    walk_embedded(schema, &path.0).ok_or_else(|| DeclarationErrorKind::UnresolvedLens {
        model,
        steps: steps.to_vec(),
    })
}

/// Walk a lens path to the physical column it names.
///
/// Two sources, each authoritative for one thing:
///
/// - the **app schema** drives the traversal, because it is the only side that knows a field is a
///   `#[document]` — a *primitive* whose storage is a model, so its inner fields collapse into the
///   one column named after it;
/// - the **compiled mapping** names the column. `Db::schema().mapping` records, per model, which
///   column every field resolves to — flattened embedded structs, enum discriminant and payload
///   columns, shared columns, and a document's single column alike — so this reads the name off
///   `db::Table` rather than re-deriving Toasty's naming rules.
///
/// Two roots reach here: a **model** root for a plain path (`seo.title`), and a **variant** root
/// for an enum payload accessor (`media.video().poster().url()`), where the generated accessor
/// rebases onto the variant and the projection's steps are variant-local.
///
/// Only embedded steps are followed. A relation hop yields `None`: this exists for embedded
/// binding, and binding anything else would reintroduce the misbind the single-segment rule guards
/// against.
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
        // An enum payload: resolve the parent path (it ends at the enum field), then descend into
        // the variant the accessor selected.
        PathRoot::Variant { parent, variant_id } => {
            let root = schema
                .app
                .get_model(parent.root.as_model_unwrap())?
                .as_root()?;
            let model = schema.mapping.models.get(&root.id)?;
            let parent_steps = parent.projection.as_slice();
            // The app side supplies the enum's payload field list, the mapping side its
            // per-variant column mappings.
            let app_field = app_field_at(schema, &root.fields, parent_steps)?;
            let Some(app::Model::EmbeddedEnum(e)) = app_embedded(schema, app_field) else {
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

/// An embedded enum's discriminant column and its variants.
///
/// A variant carries two things, and they are not interchangeable: the **value** its discriminant
/// column stores (`2`, or a string discriminant's own text), which is the form's transport, and
/// the **name** a person reads (`Published`), which is the variant control's label. The name is
/// humanized into sentence case (`In progress`), as a derived field label is; it is a label, never
/// a handle, since the normalization is lossy (`OK` reads `Ok`). Code addresses variants by
/// **declaration index**, the handle the schema itself uses (`VariantId { index }`).
#[derive(Debug, Clone)]
pub(crate) struct EnumShape {
    /// The discriminant column the form carries (`publication`).
    pub(crate) discriminant: String,
    /// Each variant's stored value and name, in declaration order.
    pub(crate) variants: Vec<(String, String)>,
}

/// The discriminant column and variants of the embedded **enum** at `path`, or `None` when `path`
/// names anything else.
///
/// [`embedded_leaf`] answers "which column does this one leaf occupy"; this answers "which column
/// carries the variant, and what does each variant store there", which is what an embedded enum's
/// variant control needs. Its payload leaves resolve one at a time through [`embedded_leaf`].
///
/// `path` addresses the embedded field itself (`Post::fields().publication()`), not one of its
/// leaves: a variant-rooted path names a *variant* of a value, not the value, and yields `None`.
pub(crate) fn embedded_enum(schema: &AppSchema, path: &ModelPath) -> Option<EnumShape> {
    let AppSchema(schema) = schema;
    let PathRoot::Model(id) = path.0.root else {
        return None;
    };
    let root = schema.app.get_model(id)?.as_root()?;
    let mapping = schema.mapping.models.get(&root.id)?;
    let steps = path.steps();
    let app_field = app_field_at(schema, &root.fields, steps)?;
    let (app::Model::EmbeddedEnum(e), MappingField::Enum(me)) = (
        app_embedded(schema, app_field)?,
        mapping_field_at(&mapping.fields, steps)?,
    ) else {
        return None;
    };
    let discriminant = column_of(schema, &MappingField::Primitive(me.discriminant.clone()))?.name;
    let variants = e
        .variants
        .iter()
        .map(|v| {
            super::value_text(&v.discriminant)
                .map(|value| (value, capitalize(&v.name.snake_case())))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(EnumShape {
        discriminant,
        variants,
    })
}

/// A `mapping::Field`, aliased so the traversal signatures stay readable.
type MappingField = mapping::Field;

/// Walk the path down to the column that stores it.
///
/// `app_fields` and `mapping_fields` are indexed by the same field index, so every step reads
/// both: the app side decides *whether to descend*, the mapping side names *the column*. Neither
/// alone is enough — the mapping cannot say a field is a `#[document]`, and the app schema cannot
/// say which column a leaf occupies without re-deriving Toasty's naming.
fn descend<F>(
    schema: &toasty_core::Schema,
    app_fields: &[F],
    mapping_fields: &[MappingField],
    steps: &[usize],
) -> Option<LeafField>
where
    F: std::borrow::Borrow<app::Field>,
{
    let (first, rest) = steps.split_first()?;
    let app_field: &app::Field = app_fields.get(*first)?.borrow();
    let mapping_field = mapping_fields.get(*first)?;
    // A `#[document]`'s inner fields share its one column, so reaching the document is reaching
    // the leaf, however many steps remain.
    if rest.is_empty() || is_document(app_field) {
        return column_of(schema, mapping_field);
    }
    let app::FieldTy::Embedded(embedded) = &app_field.ty else {
        // A relation hop, or a primitive with steps left over: not an embedded leaf either way.
        return None;
    };
    match schema.app.get_model(embedded.target)? {
        app::Model::EmbeddedStruct(e) => {
            let MappingField::Struct(ms) = mapping_field else {
                return None;
            };
            descend(schema, &e.fields, &ms.fields, rest)
        }
        app::Model::EmbeddedEnum(e) => {
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
        // A path stopping on a struct or enum names no single column, and a relation stores none.
        return None;
    };
    let column = &schema.db.tables[p.column.table.0].columns[p.column.index];
    Some(LeafField {
        name: column.name.clone(),
        label: capitalize(&column.name.replace('_', " ")),
        // Reported nullable by policy, not read off the column: only the matching enum variant
        // writes a variant payload's column. The compiled column can be `NOT NULL` — an embedded
        // struct's flattened column is.
        nullable: true,
        unique: false,
    })
}

/// Whether `field` is a `#[document]`: a *primitive* whose storage is a model, so its inner fields
/// collapse into the one column named after it.
fn is_document(field: &app::Field) -> bool {
    matches!(
        &field.ty,
        app::FieldTy::Primitive(p) if matches!(p.ty, toasty::stmt::Type::Model(_))
    )
}

/// The embedded model an app field targets, if it is embedded.
fn app_embedded<'a>(schema: &'a toasty_core::Schema, field: &app::Field) -> Option<&'a app::Model> {
    let app::FieldTy::Embedded(embedded) = &field.ty else {
        return None;
    };
    schema.app.get_model(embedded.target)
}

/// The app-level field `steps` reaches, used by the variant root to find the enum's payload list
/// before descending.
///
/// Recurses through embedded structs: the variant root's parent path can walk through them (an
/// embedded struct holding the enum), not just one step.
fn app_field_at<'a>(
    schema: &'a toasty_core::Schema,
    fields: &'a [app::Field],
    steps: &[usize],
) -> Option<&'a app::Field> {
    let (first, rest) = steps.split_first()?;
    let field = fields.get(*first)?;
    if rest.is_empty() {
        return Some(field);
    }
    match app_embedded(schema, field)? {
        app::Model::EmbeddedStruct(e) => app_field_at(schema, &e.fields, rest),
        // An enum nested in the parent path consumes two steps per level: the variant index, then
        // a variant-local field index. `variant_fields` returns the variant's own slice of the
        // enum's global field list (upstream 7ff180db), so the second step indexes it directly —
        // the generated accessor's index is variant-local, which is the same field either way.
        //
        // The variant step is bounds-checked here first: `variant_fields` indexes `variants[i]`
        // and panics out of range, and this walk answers `None` for a path it cannot resolve (a
        // wrong lens must not abort a request).
        app::Model::EmbeddedEnum(e) => {
            let (variant, tail) = rest.split_first()?;
            e.variants.get(*variant)?;
            let field = e.variant_fields(*variant).get(*tail.first()?)?;
            if tail.len() == 1 {
                Some(field)
            } else {
                app_field_at(schema, std::slice::from_ref(field), &tail[1..])
            }
        }
        app::Model::Root(_) => None,
    }
}

/// The mapping field `steps` reaches, used by the variant root to find the enum's per-variant
/// mappings before descending.
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

#[cfg(test)]
mod tests;
