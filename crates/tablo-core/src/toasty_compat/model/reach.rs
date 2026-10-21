//! The models a model's includes load, read off the app schema: through the relations of its
//! root, of the models those reach, and of the embedded values and enum variants it holds.

use toasty::schema::app;
use toasty_core::stmt::{Path, PathRoot};

use super::{AppSchema, ModelId, app_embedded, fields, lookup::app_field_at};
use crate::toasty_compat::UntypedInclude;

/// The models `includes` load off `M`'s rows, each with its name: every model a step of an include
/// reaches. `None` when a step names no field the schema holds, or `M`'s models do not compile, so
/// the caller cannot tell what the rows load.
pub(crate) fn included_models<M: toasty::schema::Model>(
    includes: &[UntypedInclude],
) -> Option<Vec<(ModelId, String)>> {
    let schema = AppSchema::of_model::<M>().ok()?;
    let schema = schema.inner();
    let mut models = Vec::new();
    for include in includes {
        let (fields, steps) = start(schema, &include.path)?;
        walk(schema, fields, steps, &mut models)?;
    }
    Some(models)
}

/// The fields an include path's steps index, and the steps: the root model's fields for a path
/// from a model, or a variant's payload for a path from an enum variant, whose steps are local.
fn start<'a>(
    schema: &'a toasty_core::Schema,
    path: &'a Path,
) -> Option<(&'a [app::Field], &'a [usize])> {
    match &path.root {
        PathRoot::Model(model) => {
            let root = schema.app.get_model(*model)?.as_root()?;
            Some((&root.fields, path.projection.as_slice()))
        }
        PathRoot::Variant { parent, variant_id } => {
            let (fields, steps) = start(schema, parent)?;
            let field = app_field_at(schema, fields, steps)?;
            let app::Model::EmbeddedEnum(enum_) = app_embedded(schema, field)? else {
                return None;
            };
            enum_.variants.get(variant_id.index)?;
            Some((
                enum_.variant_fields(variant_id.index),
                path.projection.as_slice(),
            ))
        }
    }
}

/// Walks `steps` through `fields`, recording each model a relation they reach loads.
fn walk(
    schema: &toasty_core::Schema,
    fields: &[app::Field],
    steps: &[usize],
    models: &mut Vec<(ModelId, String)>,
) -> Option<()> {
    let Some((first, rest)) = steps.split_first() else {
        return Some(());
    };
    let field = fields.get(*first)?;
    match &field.ty {
        app::FieldTy::BelongsTo(relation) => reach(schema, relation.target, rest, models),
        app::FieldTy::Has(relation) => reach(schema, relation.target, rest, models),
        app::FieldTy::Via(relation) => reach(schema, relation.target, rest, models),
        app::FieldTy::Primitive(_) => rest.is_empty().then_some(()),
        app::FieldTy::Embedded(_) => match app_embedded(schema, field)? {
            app::Model::EmbeddedStruct(embedded) => walk(schema, &embedded.fields, rest, models),
            app::Model::EmbeddedEnum(enum_) => {
                // The enum's own value holds no relation: only a step into a variant reaches one.
                let Some((variant, tail)) = rest.split_first() else {
                    return Some(());
                };
                enum_.variants.get(*variant)?;
                walk(schema, enum_.variant_fields(*variant), tail, models)
            }
            app::Model::Root(_) => None,
        },
    }
}

/// Records the model a relation loads, then walks the steps past it through its fields.
fn reach(
    schema: &toasty_core::Schema,
    target: ModelId,
    steps: &[usize],
    models: &mut Vec<(ModelId, String)>,
) -> Option<()> {
    let model = schema.app.get_model(target)?;
    models.push((target, model.name().upper_camel_case()));
    match model.as_root() {
        Some(root) => walk(schema, &root.fields, steps, models),
        None => steps.is_empty().then_some(()),
    }
}

/// The models the root fields `names` reach through a relation, each with its name: a relation
/// field reaches its target, and an embedded value reaches each relation it holds. `None` when a
/// name is no field of `M` the schema holds.
pub(crate) fn read_relations<M: toasty::schema::Model>(
    names: &[String],
) -> Option<Vec<(ModelId, String)>> {
    let schema = AppSchema::of_model::<M>().ok()?;
    let schema = schema.inner();
    let root = schema.app.get_model(M::id())?.as_root()?;
    let mut models = Vec::new();
    for name in names {
        let info = fields::<M>().into_iter().find(|info| &info.name == name)?;
        reach_field(schema, root.fields.get(info.index)?, &mut models)?;
    }
    Some(models)
}

/// Records the models `field` reaches through a relation it is, or holds.
fn reach_field(
    schema: &toasty_core::Schema,
    field: &app::Field,
    models: &mut Vec<(ModelId, String)>,
) -> Option<()> {
    match &field.ty {
        app::FieldTy::BelongsTo(relation) => reach(schema, relation.target, &[], models),
        app::FieldTy::Has(relation) => reach(schema, relation.target, &[], models),
        app::FieldTy::Via(relation) => reach(schema, relation.target, &[], models),
        app::FieldTy::Primitive(_) => Some(()),
        app::FieldTy::Embedded(_) => match app_embedded(schema, field)? {
            app::Model::EmbeddedStruct(embedded) => embedded
                .fields
                .iter()
                .try_for_each(|field| reach_field(schema, field, models)),
            app::Model::EmbeddedEnum(enum_) => enum_
                .fields
                .iter()
                .try_for_each(|field| reach_field(schema, field, models)),
            app::Model::Root(_) => None,
        },
    }
}
