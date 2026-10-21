//! The app and mapping fields a path's steps reach.

use toasty::schema::app;

use super::{MappingField, app_embedded};

/// The app-level field `steps` reaches, used by the variant root to find the enum's payload list
/// before descending.
///
/// Recurses through embedded structs: the variant root's parent path can walk through them (an
/// embedded struct holding the enum), not just one step.
pub(super) fn app_field_at<'a>(
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
        // enum's global field list, so the second step indexes it directly —
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
pub(super) fn mapping_field_at<'a>(
    fields: &'a [MappingField],
    steps: &[usize],
) -> Option<&'a MappingField> {
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
