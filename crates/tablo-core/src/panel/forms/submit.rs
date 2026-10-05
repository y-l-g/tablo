//! The create/edit POST pipelines.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use topcoat::{
    Result,
    context::Cx,
    router::{Body, error::forbidden},
    view::BoxView,
};

use super::{
    super::{actions::find_by_key, gate::gate, write::commit_write},
    common::{
        FormParts, drop_client_typed_uploads, reject_unknown_form_keys, rerender_invalid_form,
        restore_pending_uploads, strip_transport_keys, truthy,
    },
    decode::parse_form_body,
    render::FormChrome,
    unique::check_unique,
};
use crate::{
    db::db,
    error::TabloError,
    form::{FieldErrorKind, FieldErrors, Posted, RecordForm},
    policy::{Ability, can},
    resource::{Committed, Resource, declared},
    schema::Schema,
    topcoat_compat::async_page,
};

const WRITE_CREATE: &str = "create the record";
const WRITE_UPDATE: &str = "save the changes";

struct Submission {
    schema: Arc<Schema>,
    values: HashMap<String, String>,
    errors: FieldErrors,
    carried: HashSet<String>,
    named: HashSet<String>,
}

/// Stages a create/edit submission, completing unnamed keys from the edit advisory snapshot.
async fn prepare_submission<R: Resource>(
    cx: &Cx,
    parts: FormParts,
    advisory: Option<&R::Model>,
) -> Result<Submission, topcoat::Error> {
    let schema = Arc::clone(&declared::<R>(cx).form);
    reject_unknown_form_keys(&schema, &parts.values)?;
    let FormParts {
        mut values,
        files,
        file_part_names,
    } = parts;
    let stored = advisory
        .map(|advisory| <R::Form as RecordForm>::hydrate(cx, advisory))
        .unwrap_or_default();
    // File fields take values only from file parts.
    drop_client_typed_uploads(&schema, &file_part_names, &mut values);
    let (upload_errors, mut carried) =
        crate::upload::store_uploads(cx, &schema, &files, &mut values).await;
    carried.extend(restore_pending_uploads(cx, &schema, &mut values).await);
    // An empty file input keeps the stored value; `clear_<field>` clears it unless a file was
    // chosen.
    for field in schema.fields().filter(|field| field.is_file()) {
        let name = field.name();
        let cleared = values
            .get(&format!("clear_{name}"))
            .is_some_and(|v| truthy(v));
        let empty = values
            .get(name)
            .map(|v| v.trim().is_empty())
            .unwrap_or(true);
        if !cleared && empty && stored.get(name).is_some_and(|v| !v.trim().is_empty()) {
            values.remove(name);
        }
    }
    strip_transport_keys(&schema, &mut values);
    let named: HashSet<String> = values.keys().cloned().collect();
    complete(&schema, &mut values, &named, &stored);
    let mut errors = schema.validate_async(cx, &values).await;
    // A rejected upload owns its field's error slot.
    errors.replace(upload_errors);
    Ok(Submission {
        schema,
        values,
        errors,
        carried,
        named,
    })
}

/// Fills unnamed keys from the stored projection, dropping keys it does not hold.
fn complete(
    schema: &Schema,
    values: &mut HashMap<String, String>,
    named: &HashSet<String>,
    stored: &HashMap<String, String>,
) {
    for field in schema.fields() {
        let name = field.name();
        if named.contains(name) {
            continue;
        }
        match stored.get(name) {
            Some(value) => values.insert(name.to_string(), value.clone()),
            None => values.remove(name),
        };
    }
}

/// Parses completed values and runs `validate_record`, refusing keys this submission renders
/// nowhere.
fn parse_form<R: Resource>(
    cx: &Cx,
    schema: &Schema,
    values: &HashMap<String, String>,
    errors: &mut FieldErrors,
) -> Result<Option<R::Form>, topcoat::Error> {
    let mut normalized = values.clone();
    schema.normalize_values(&mut normalized);
    match <R::Form as RecordForm>::parse(cx, &normalized) {
        Ok(form) => {
            for error in R::validate_record(cx, &form).iter() {
                if !schema.renders_error_key(values, &error.key) {
                    return Err(unrenderable_error::<R>(
                        "validate_record",
                        &error.key,
                        &error.message,
                    ));
                }
                errors.push(error.clone());
            }
            Ok(Some(form))
        }
        Err(failures) => {
            let controls = schema.controls();
            for mut failure in failures {
                if !schema.renders_error_key(values, &failure.key) {
                    return Err(unrenderable_error::<R>(
                        "the parse",
                        &failure.key,
                        &failure.message,
                    ));
                }
                if errors.contains_key(&failure.key) {
                    continue;
                }
                if failure.kind == FieldErrorKind::Required
                    && let Some(wording) = controls
                        .iter()
                        .find(|control| control.name == failure.key)
                        .and_then(|control| control.required_error.clone())
                {
                    failure.message = wording;
                }
                errors.push(failure);
            }
            Ok(None)
        }
    }
}

/// Re-checks relationship keys inside the write transaction before writing.
async fn recheck_relationships(
    cx: &Cx,
    schema: &Schema,
    values: &HashMap<String, String>,
    errors: &mut FieldErrors,
    ex: &mut dyn toasty::Executor,
) {
    if errors.is_empty() {
        errors.extend(schema.recheck_relationships(cx, values, ex).await);
    }
}

/// Refuse an error whose key this submission renders nowhere: no slot would
/// carry the message, and writing anyway would drop it.
fn unrenderable_error<R: Resource>(source: &str, key: &str, message: &str) -> topcoat::Error {
    TabloError::Declaration(format!(
        "{source} refused {key:?}, which `{}` renders nowhere for this submission: {message}",
        std::any::type_name::<R::Form>()
    ))
    .into()
}

fn named_fields<R: Resource>(
    cx: &Cx,
    named: &HashSet<String>,
) -> Vec<<R::Form as RecordForm>::Field> {
    declared::<R>(cx)
        .fields
        .iter()
        .filter(|field| field.keys.iter().any(|key| named.contains(key)))
        .map(|field| field.field)
        .collect()
}

pub(crate) fn resource_create_post<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        gate::<R>(cx)?;
        if !can::<R>(cx, Ability::Create) {
            return Err(forbidden().into());
        }
        let parts = parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, &parts.values)?;
        let Submission {
            schema,
            values,
            mut errors,
            carried,
            ..
        } = prepare_submission::<R>(cx, parts, None).await?;
        // Opens the transaction after validation so loaders never block on the held connection.
        let mut db = db(cx);
        let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
        // App-side unique check; toasty exposes no unique-violation predicate yet (#117).
        errors.extend(check_unique::<R>(cx, &schema, &values, &HashMap::new(), &mut tx).await?);
        let form = parse_form::<R>(cx, &schema, &values, &mut errors)?;
        recheck_relationships(cx, &schema, &values, &mut errors, &mut tx).await;
        let Some(form) = form.filter(|_| errors.is_empty()) else {
            return rerender_invalid_form::<R>(
                cx,
                tx,
                FormChrome::create::<R>(),
                &values,
                &errors,
                &carried,
            )
            .await;
        };
        let written = R::create_record(cx, form, &mut tx).await;
        commit_write::<R, _>(cx, tx, written, Committed::created, "Created", WRITE_CREATE).await
    })
}

/// Validates the edit submission and writes named fields, requiring both `View` and `Update`.
pub(crate) fn resource_edit_post<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        gate::<R>(cx)?;
        let parts = parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, &parts.values)?;
        let id = topcoat::router::path_param_segment(cx, "id").to_string();
        // Advisory load feeds validation; the authoritative load runs inside the transaction.
        let mut db0 = db(cx);
        let advisory = find_by_key::<R>(cx, &id, &mut db0).await?;
        if !can::<R>(cx, Ability::View(&advisory)) {
            return Err(forbidden().into());
        }
        if !can::<R>(cx, Ability::Update(&advisory)) {
            return Err(forbidden().into());
        }
        let Submission {
            schema,
            mut values,
            mut errors,
            carried,
            named,
        } = prepare_submission::<R>(cx, parts, Some(&advisory)).await?;
        // Authoritative load inside the transaction observes the write snapshot (#86).
        let mut db = db(cx);
        let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
        let record = find_by_key::<R>(cx, &id, &mut tx).await?;
        if !can::<R>(cx, Ability::View(&record)) {
            return Err(forbidden().into());
        }
        if !can::<R>(cx, Ability::Update(&record)) {
            return Err(forbidden().into());
        }
        // Unnamed keys complete from this snapshot and are never written back.
        let stored = <R::Form as RecordForm>::hydrate(cx, &record);
        complete(&schema, &mut values, &named, &stored);
        errors.extend(check_unique::<R>(cx, &schema, &values, &stored, &mut tx).await?);
        let form = parse_form::<R>(cx, &schema, &values, &mut errors)?;
        recheck_relationships(cx, &schema, &values, &mut errors, &mut tx).await;
        let Some(form) = form.filter(|_| errors.is_empty()) else {
            return rerender_invalid_form::<R>(
                cx,
                tx,
                FormChrome::edit::<R>(cx, &record),
                &values,
                &errors,
                &carried,
            )
            .await;
        };
        let posted = Posted::new(form, named_fields::<R>(cx, &named));
        let written = R::update_record(cx, record, posted, &mut tx).await;
        commit_write::<R, _>(cx, tx, written, Committed::updated, "Updated", WRITE_UPDATE).await
    })
}

#[cfg(test)]
mod tests;
