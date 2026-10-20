//! The models a model's includes load, read off the app schema.

use toasty::schema::app;
use toasty_core::stmt::PathRoot;

use super::{AppSchema, ModelId};
use crate::toasty_compat::UntypedInclude;

/// The models `includes` load off `M`'s rows, each with its name: every model a step of an
/// include reaches, a nested include's included. Empty when `M`'s models do not compile.
pub(crate) fn included_models<M: toasty::schema::Model>(
    includes: &[UntypedInclude],
) -> Vec<(ModelId, String)> {
    let Ok(schema) = AppSchema::of_model::<M>() else {
        return Vec::new();
    };
    let app = &schema.inner().app;
    let mut models = Vec::new();
    for include in includes {
        let PathRoot::Model(mut model) = include.path.root else {
            continue;
        };
        for &step in include.path.projection.as_slice() {
            let Some(field) = app
                .get_model(model)
                .and_then(|model| model.as_root())
                .and_then(|root| root.fields.get(step))
            else {
                break;
            };
            let target = match &field.ty {
                app::FieldTy::BelongsTo(relation) => relation.target,
                app::FieldTy::Has(relation) => relation.target,
                app::FieldTy::Via(relation) => relation.target,
                _ => break,
            };
            let Some(reached) = app.get_model(target) else {
                break;
            };
            models.push((target, reached.name().upper_camel_case()));
            model = target;
        }
    }
    models
}
