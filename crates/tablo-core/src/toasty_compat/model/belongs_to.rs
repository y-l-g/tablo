//! A model's `belongs_to` relations, read off the app schema.

use toasty::schema::app;

use super::ModelId;

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

/// Every `belongs_to` relation of `M` that names a foreign key, in field order.
pub(crate) fn belongs_tos<M: toasty::schema::Model>() -> Vec<BelongsTo> {
    let count = M::schema().as_root().map_or(0, |root| root.fields.len());
    (0..count).filter_map(belongs_to::<M>).collect()
}
