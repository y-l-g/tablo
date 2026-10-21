//! App-side uniqueness probe over `unique()`-marked text fields.

use std::collections::HashSet;

use topcoat::{Result, context::Cx};

use crate::{
    db::db,
    form::{FieldErrors, FormValues},
    resource::{Mounted, Resource, Trashed},
};

/// Refuses duplicated `unique()` values with `<Label> has already been taken`, skipping empty
/// submits and the record's own value (tablo#117).
///
/// A composite index is probed on all the fields the form posts, each through the value its own
/// text field posts. A field the form has no control for, the tenant column among them, is left to
/// the scoped query, as [`Resource::query`]'s scope fixes it. An index with a field whose control
/// posts no value, or is not a text field, is left to the database.
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
                .filter(|other| schema.fields().any(|field| field.name() == *other))
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
            // Probes inside the write transaction through the tenant-scoped query, trashed records
            // included: the index covers them.
            let rows = resource
                .scoped_query_in(cx, Trashed::With)?
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

/// The submitted unique values another record holds now that the database refused a write with
/// `error`, read once the write's transaction is gone: empty when none is taken, when the probe
/// fails too, or when `error` is the app's own rather than Toasty's.
///
/// [`check_unique`] probes inside the write's transaction, so it cannot see a concurrent write
/// of the same value that has not committed; the unique index then refuses the write. Toasty
/// names no unique violation (tablo#117), but on PostgreSQL and MySQL the index refuses this
/// write only once the other has committed, so the probe sees its row. SQLite can refuse it as
/// busy before the other commits, and the probe then finds nothing.
pub(super) async fn taken_since<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    schema: &crate::schema::Schema,
    values: &FormValues,
    current: &FormValues,
    error: &topcoat::Error,
) -> FieldErrors {
    if !error.is::<toasty::Error>() {
        return FieldErrors::new();
    }
    check_unique(cx, resource, schema, values, current, &mut db(cx))
        .await
        .unwrap_or_default()
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
