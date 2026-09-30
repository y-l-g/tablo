//! App-side uniqueness probe over `unique()`-marked text fields.

use std::collections::HashMap;

use topcoat::{Result, context::Cx};

use crate::resource::Resource;

/// App-side uniqueness check over the form's `unique()`-marked text fields.
///
/// Generic over every marked field. Queries through the tenant-scoped query and
/// returns `field_name → ["<Label> has already been taken"]` per duplicated
/// value. `current` holds the record's own hydrated values on edit: a field
/// whose submitted value normalises to the same stored value belongs to this
/// record and is skipped, so a typed field's re-spelled equivalent is not a
/// duplicate.
///
/// Empty submits are never probed: a `unique()` field is required (see
/// [`crate::schema::Field::unique`]), so `validate` has already answered
/// `"<Label> is required"` and this check has nothing left to say.
///
/// The probe binds the leaf's own type: a typed field parses the submission and
/// compares the parsed value, so a value unique as text but not as its declared
/// type is still refused.
///
/// Known limits (upstream gap #117): races with concurrent inserts, and a
/// `unique()` field whose index carries components outside the tenant-scoped
/// query's scope is not checked exactly — a composite index such as
/// `#[unique(tenant_id, email)]` on a tenant-scoped resource is. `unique`
/// exists on text fields only.
pub(super) async fn check_unique<R: Resource>(
    cx: &Cx,
    schema: &crate::schema::Schema,
    values: &HashMap<String, String>,
    current: &HashMap<String, String>,
    ex: &mut dyn toasty::Executor,
) -> Result<HashMap<String, Vec<String>>, topcoat::Error> {
    let mut errors: HashMap<String, Vec<String>> = HashMap::new();
    // Groups the submission leaves out are not checked:
    // `validate` treats an all-empty repeater group and a hidden variant group
    // as untouched through the same classification, so a stored value must not
    // flag a group the user never saw.
    let skip = schema.absent_fields(values);
    for field in schema.fields() {
        let name = field.name();
        if !field.is_unique() || skip.contains(name) {
            continue;
        }
        let Some(submitted) = values.get(name).map(|s| s.trim().to_string()) else {
            continue;
        };
        // Empty values are never probed: a `unique` field is
        // required, so validation has already refused this submit — and `""` is
        // still a value the framework stores (never NULL), so a probe
        // would only rediscover the constraint the form just enforced.
        if submitted.is_empty() {
            continue;
        }
        // Unchanged on edit → this record's own value, not a duplicate. Both
        // sides normalise through the leaf's own rule: a typed
        // field's re-spelled equivalent — `01` for `1`, an upper-case UUID for
        // its lower-case form — is the same value, so the probe is skipped. A
        // text comparison would call it changed, probe this record's own row
        // and refuse the save.
        let unchanged = current.get(name).is_some_and(|kept| {
            matches!(
                (field.normalize(kept), field.normalize(&submitted)),
                (Ok(kept), Ok(submitted)) if kept == submitted
            )
        });
        if unchanged {
            continue;
        }
        // The leaf's own binding: a typed field parses the
        // submission first, so the probe compares the value the record will
        // store rather than its spelling. A typed submission that does not
        // parse has no value to compare — validation refused it first.
        let Some(filter) = field.eq_filter(&submitted) else {
            continue;
        };
        // Inside the handler's tx: the check observes the same
        // snapshot as the write that follows. A failing probe fails the
        // submit — swallowing it would write past a check that
        // never ran. The probe runs through the tenant-scoped query and reads
        // only the record's own columns.
        let rows = crate::resource::scoped_query::<R>(cx)?
            .filter(filter)
            .limit(1)
            .exec(&mut *ex)
            .await
            .map_err(crate::db::unavailable)?;
        if !rows.is_empty() {
            errors.insert(
                name.to_string(),
                vec![format!("{} has already been taken", field.label_str())],
            );
        }
    }
    Ok(errors)
}
#[cfg(test)]
mod tests;
