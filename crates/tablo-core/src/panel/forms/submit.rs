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
    super::{actions::load_editable, gate::gate, write::commit_write},
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
    form::{FieldError, FieldErrors, Posted, RecordForm},
    policy::Ability,
    resource::{Committed, Mounted, Resource},
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
    resource: &Mounted<R>,
    parts: FormParts,
    advisory: Option<&R::Model>,
) -> Result<Submission, topcoat::Error> {
    let schema = Arc::clone(&resource.form);
    let FormParts {
        mut values,
        lists,
        mut files,
        file_part_names,
    } = parts;
    // Each repeater's rows, and each multiple choice's values, become its one key, which every
    // step below reads like any other.
    schema
        .fold_repeaters(&mut values)
        .map_err(topcoat::router::error::bad_request)?;
    schema.fold_choices(&mut values, &lists);
    reject_unknown_form_keys(&schema, &values)?;
    let stored = advisory
        .map(|advisory| <R::Form as RecordForm>::hydrate(cx, advisory))
        .unwrap_or_default();
    // A hidden field's control is disabled, so the browser posts nothing for it: a key posted
    // anyway is dropped, with a file field's carried upload, so an edit keeps the stored value and
    // a create takes the blank answer. An edit that does not post the watched field reads its
    // stored value, as the parse will.
    let mut read = stored.clone();
    read.extend(
        values
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    for key in schema.condition_hidden(&read) {
        values.remove(&format!("keep_{key}"));
        values.remove(&format!("clear_{key}"));
        values.remove(&key);
        files.remove(&key);
    }
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
    let mut errors = FieldErrors::new();
    schema
        .check_controls(cx, &values, &stored, &mut errors)
        .await;
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

/// Parses completed values and runs `validate_record` on the parsed form, adding every refusal
/// to `errors` under the form key it renders under.
///
/// # Errors
///
/// A declaration error when a hand-written record form refuses a key its `fields` do not bind:
/// no control would carry the message, and writing anyway would drop it.
fn parse_form<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    values: &HashMap<String, String>,
    errors: &mut FieldErrors,
) -> Result<Option<R::Form>, topcoat::Error> {
    let unbound = |source: &str, key: &dyn std::fmt::Debug| {
        crate::error::declaration(format!(
            "{source} refused {key:?}, which `{}` binds to no form key",
            std::any::type_name::<R::Form>()
        ))
    };
    match <R::Form as RecordForm>::parse(cx, values) {
        Ok(form) => {
            for FieldError { key, kind } in R::validate_record(cx, &form) {
                let Some(first) = resource
                    .fields
                    .iter()
                    .find(|field| field.field == key)
                    .and_then(|field| field.keys.first())
                else {
                    return Err(unbound("validate_record", &key));
                };
                errors.push(FieldError {
                    key: first.clone(),
                    kind,
                });
            }
            Ok(Some(form))
        }
        Err(failures) => {
            for failure in failures {
                if !resource.fields.iter().any(|field| {
                    field
                        .keys
                        .iter()
                        .any(|key| renders_under(&resource.form, key, &failure.key))
                }) {
                    return Err(unbound("the parse", &failure.key));
                }
                // The controls' own rules answered first for the key.
                if !errors.contains_key(&failure.key) {
                    errors.push(failure);
                }
            }
            Ok(None)
        }
    }
}

/// Whether an error on `failed` renders under the field posting `key`: its own, or, for a
/// repeater, a row's control, which posts under `{key}.`.
fn renders_under(form: &Schema, key: &str, failed: &str) -> bool {
    match failed.strip_prefix(key) {
        Some("") => true,
        Some(rest) => {
            rest.starts_with('.')
                && form
                    .fields()
                    .any(|field| field.name() == key && field.as_repeater().is_some())
        }
        None => false,
    }
}

/// Re-checks relationship keys inside the write transaction before writing, but the links the
/// record in `stored` holds already.
async fn recheck_relationships(
    cx: &Cx,
    schema: &Schema,
    values: &HashMap<String, String>,
    stored: &HashMap<String, String>,
    errors: &mut FieldErrors,
    ex: &mut dyn toasty::Executor,
) {
    if errors.is_empty() {
        errors.extend(schema.recheck_relationships(cx, values, stored, ex).await);
    }
}

fn named_fields<R: Resource>(
    resource: &Mounted<R>,
    named: &HashSet<String>,
) -> Vec<<R::Form as RecordForm>::Field> {
    resource
        .fields
        .iter()
        .filter(|field| field.keys.iter().any(|key| named.contains(key)))
        .map(|field| field.field)
        .collect()
}

pub(crate) fn resource_create_post<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can(cx, Ability::Create) {
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
        } = prepare_submission(cx, &resource, parts, None).await?;
        // Opens the transaction after validation so loaders never block on the held connection.
        let mut db = db(cx);
        let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
        // App-side unique check; toasty exposes no unique-violation predicate yet (#117).
        errors
            .extend(check_unique(cx, &resource, &schema, &values, &HashMap::new(), &mut tx).await?);
        let form = parse_form(cx, &resource, &values, &mut errors)?;
        recheck_relationships(cx, &schema, &values, &HashMap::new(), &mut errors, &mut tx).await;
        let Some(form) = form.filter(|_| errors.is_empty()) else {
            return rerender_invalid_form(
                cx,
                &resource,
                tx,
                FormChrome::create(&resource),
                &values,
                &errors,
                &carried,
            )
            .await;
        };
        let written = R::create_record(cx, form, &mut tx).await;
        commit_write(
            cx,
            &resource,
            tx,
            written,
            Committed::created,
            "Created",
            WRITE_CREATE,
        )
        .await
    })
}

/// Validates the edit submission and writes named fields, requiring both `View` and `Update`.
pub(crate) fn resource_edit_post<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let parts = parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, &parts.values)?;
        // Advisory load feeds validation; the authoritative load runs inside the transaction.
        let mut db0 = db(cx);
        let advisory = load_editable(cx, &resource, &mut db0).await?;
        if !resource.can(cx, Ability::Update(&advisory)) {
            return Err(forbidden().into());
        }
        let Submission {
            schema,
            mut values,
            mut errors,
            carried,
            named,
        } = prepare_submission(cx, &resource, parts, Some(&advisory)).await?;
        // Authoritative load inside the transaction observes the write snapshot (#86).
        let mut db = db(cx);
        let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
        let record = load_editable(cx, &resource, &mut tx).await?;
        if !resource.can(cx, Ability::Update(&record)) {
            return Err(forbidden().into());
        }
        // Unnamed keys complete from this snapshot and are never written back.
        let stored = <R::Form as RecordForm>::hydrate(cx, &record);
        complete(&schema, &mut values, &named, &stored);
        schema
            .keep_unseen_links(cx, &mut values, &stored, &mut tx)
            .await;
        errors.extend(check_unique(cx, &resource, &schema, &values, &stored, &mut tx).await?);
        let form = parse_form(cx, &resource, &values, &mut errors)?;
        recheck_relationships(cx, &schema, &values, &stored, &mut errors, &mut tx).await;
        let Some(form) = form.filter(|_| errors.is_empty()) else {
            return rerender_invalid_form(
                cx,
                &resource,
                tx,
                FormChrome::edit(cx, &resource, &record),
                &values,
                &errors,
                &carried,
            )
            .await;
        };
        let posted = Posted::new(form, named_fields(&resource, &named));
        let written = R::update_record(cx, record, posted, &mut tx).await;
        commit_write(
            cx,
            &resource,
            tx,
            written,
            Committed::updated,
            "Updated",
            WRITE_UPDATE,
        )
        .await
    })
}

#[cfg(test)]
mod tests;
