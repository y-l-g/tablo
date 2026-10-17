//! App-side uniqueness probe over `unique()`-marked text fields.

use std::collections::HashSet;

use topcoat::{Result, context::Cx};

use crate::{
    form::{FieldErrors, FormValues},
    resource::{Mounted, Resource},
};

/// Refuses duplicated `unique()` values with `<Label> has already been taken`, skipping empty
/// submits and the record's own value (tablo#117).
///
/// A composite index is probed on all its fields: the tenant column through the tenant-scoped
/// query, and every other one through the value its own text field posts. An index with a field
/// no text field posts, or posting no value, is left to the database.
pub(super) async fn check_unique<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    schema: &crate::schema::Schema,
    values: &FormValues,
    current: &FormValues,
    ex: &mut dyn toasty::Executor,
) -> Result<FieldErrors, topcoat::Error> {
    let mut errors = FieldErrors::new();
    // Skips the variant groups the submission hides.
    let skip = schema.hidden_fields(values);
    let tenant = resource
        .tenancy
        .column_field()
        .and_then(Result::ok)
        .map(|field| field.name.as_str());
    // A composite index two text fields post is probed once, for the first.
    let mut probed = HashSet::new();
    for field in schema.fields() {
        let name = field.name();
        if !field.is_unique() || skip.contains(name) {
            continue;
        }
        for others in field.unique_with() {
            let mut members: Vec<&str> = others
                .iter()
                .map(String::as_str)
                .filter(|other| Some(*other) != tenant)
                .chain([name])
                .collect();
            members.sort_unstable();
            if probed.contains(&members) {
                continue;
            }
            let Some(filter) = probe(schema, &skip, values, current, &members) else {
                continue;
            };
            probed.insert(members);
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
                break;
            }
        }
    }
    Ok(errors)
}

/// The filter matching another row that holds the submitted value of every field in `members`,
/// or `None` when one has no value to probe, or every one spells the record's own stored value.
fn probe(
    schema: &crate::schema::Schema,
    skip: &HashSet<String>,
    values: &FormValues,
    current: &FormValues,
    members: &[&str],
) -> Option<toasty::stmt::Expr<bool>> {
    let mut filter: Option<toasty::stmt::Expr<bool>> = None;
    let mut unchanged = true;
    for member in members {
        let field = schema.fields().find(|field| field.name() == *member)?;
        if skip.contains(*member) {
            return None;
        }
        // Never probes empty values: the column is nullable or the field required.
        let submitted = values
            .text(member)
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        unchanged &= current
            .text(member)
            .is_some_and(|kept| field.same_value(kept, submitted));
        // Probes the parsed value for typed fields.
        let eq = field.eq_filter(submitted)?;
        filter = Some(match filter {
            Some(filter) => filter.and(eq),
            None => eq,
        });
    }
    filter.filter(|_| !unchanged)
}
#[cfg(test)]
mod tests;
