//! App-side uniqueness probe over `unique()`-marked text fields.

use std::collections::HashMap;

use topcoat::{Result, context::Cx};

use crate::{
    form::FieldErrors,
    resource::{Mounted, Resource},
};

/// Refuses duplicated `unique()` values with `<Label> has already been taken`, skipping empty
/// submits and the record's own value (#117).
pub(super) async fn check_unique<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    schema: &crate::schema::Schema,
    values: &HashMap<String, String>,
    current: &HashMap<String, String>,
    ex: &mut dyn toasty::Executor,
) -> Result<FieldErrors, topcoat::Error> {
    let mut errors = FieldErrors::new();
    // Skips groups the submission leaves out.
    let skip = schema.absent_fields(values);
    for field in schema.fields() {
        let name = field.name();
        if !field.is_unique() || skip.contains(name) {
            continue;
        }
        let Some(submitted) = values.get(name).map(|s| s.trim().to_string()) else {
            continue;
        };
        // Never probes empty values; validation already refused them.
        if submitted.is_empty() {
            continue;
        }
        // Skips values normalising to the record's own stored value.
        let unchanged = current.get(name).is_some_and(|kept| {
            matches!(
                (field.normalize(kept), field.normalize(&submitted)),
                (Ok(kept), Ok(submitted)) if kept == submitted
            )
        });
        if unchanged {
            continue;
        }
        // Probes the parsed value for typed fields.
        let Some(filter) = field.eq_filter(&submitted) else {
            continue;
        };
        // Probes inside the write transaction through the tenant-scoped query.
        let rows = resource
            .scoped_query(cx)?
            .filter(filter)
            .limit(1)
            .exec(&mut *ex)
            .await
            .map_err(crate::error::unavailable)?;
        if !rows.is_empty() {
            errors.add(
                name,
                format!("{} has already been taken", field.label_str()),
            );
        }
    }
    Ok(errors)
}
#[cfg(test)]
mod tests;
