//! The create/edit POST pipelines.

use std::{collections::HashSet, sync::Arc};

use topcoat::{
    Result,
    context::Cx,
    router::{Body, error::forbidden},
    view::BoxView,
};

use super::{
    super::{
        actions::load_editable,
        gate::gate,
        write::{commit_write, failed_write},
    },
    common::{
        FormParts, blank, drop_client_typed_uploads, file_flags, reject_unknown_form_keys,
        rerender_invalid_form, restore_pending_uploads,
    },
    decode::parse_form_body,
    render::{FormChrome, VERSION_FIELD, render_form_page},
    unique::{check_unique, taken_since},
};
use crate::{
    ModelColumns as _,
    db::db,
    form::{FieldError, FieldErrors, FormValues, Posted, RecordForm},
    panel::state::uploader,
    policy::Ability,
    resource::{Committed, Mounted, Resource},
    schema::Schema,
    topcoat_compat::async_page,
};

const WRITE_CREATE: &str = "create the record";
const WRITE_UPDATE: &str = "save the changes";

struct Submission {
    schema: Arc<Schema>,
    values: FormValues,
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
    // Each repeater's rows, and each multiple choice's values, become its one key, which every
    // step below reads like any other.
    let (mut values, unclaimed) = schema
        .read_unclaimed(&parts.pairs)
        .map_err(topcoat::router::error::bad_request)?;
    reject_unknown_form_keys(&schema, unclaimed)?;
    let mut flags = file_flags(&schema, &parts);
    let FormParts {
        mut files,
        file_part_names,
        ..
    } = parts;
    let stored = advisory
        .map(|advisory| <R::Form as RecordForm>::hydrate(cx, advisory))
        .unwrap_or_default();
    // A disabled control posts nothing: a key posted anyway is dropped, so an edit keeps the
    // stored value and a create takes the field's default. Conditions then read that value, as
    // the browser did.
    let editing = advisory.is_some();
    for key in schema.disabled_keys(editing) {
        files.remove(key);
        flags.remove(key);
    }
    schema.drop_disabled(&mut values, editing);
    // A hidden field's control is disabled, so the browser posts nothing for it: a key posted
    // anyway is dropped, with a file field's carried upload, so an edit keeps the stored value and
    // a create takes the blank answer. An edit that does not post the watched field reads its
    // stored value, as the parse will.
    let mut read = stored.clone();
    read.extend(values.clone());
    for key in &schema.condition_hidden(&read) {
        values.remove(key);
        files.remove(key);
        flags.remove(key);
    }
    // File fields take values only from file parts.
    drop_client_typed_uploads(&schema, &file_part_names, &mut values);
    let (upload_errors, mut carried) =
        crate::upload::store_uploads(uploader(cx), &schema, &files, &mut values).await;
    carried.extend(restore_pending_uploads(cx, &flags, &mut values).await);
    // An empty file input keeps the stored value; `clear_<field>` clears it unless a file was
    // chosen.
    for (name, flags) in &flags {
        if !flags.clear && blank(&values, name) && !blank(&stored, name) {
            values.remove(name);
        }
    }
    let named: HashSet<String> = values.keys().map(str::to_string).collect();
    complete(&schema, &mut values, &named, &stored);
    schema.keep_unedited(&mut values, &stored);
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
    values: &mut FormValues,
    named: &HashSet<String>,
    stored: &FormValues,
) {
    for field in schema.fields() {
        let name = field.name();
        if named.contains(name) {
            continue;
        }
        match stored.get(name) {
            Some(value) => values.insert(name, value.clone()),
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
    values: &FormValues,
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
    values: &FormValues,
    stored: &FormValues,
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
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
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
        // App-side unique check: Toasty exposes no unique-violation predicate (tablo#117). A
        // write the index refuses all the same re-probes through `taken_since`.
        errors.extend(
            check_unique(cx, &resource, &schema, &values, &FormValues::new(), &mut tx).await?,
        );
        let form = parse_form(cx, &resource, &values, &mut errors)?;
        recheck_relationships(
            cx,
            &schema,
            &values,
            &FormValues::new(),
            &mut errors,
            &mut tx,
        )
        .await;
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
        let written = match R::create_record(cx, form, &mut tx).await {
            Ok(written) => written,
            Err(error) => {
                drop(tx);
                let taken =
                    taken_since(cx, &resource, &schema, &values, &FormValues::new(), &error).await;
                if taken.is_empty() {
                    return Err(failed_write(cx, WRITE_CREATE, error));
                }
                return render_form_page(
                    cx,
                    &resource.form,
                    FormChrome::create(&resource),
                    &values,
                    &taken,
                    &carried,
                )
                .await;
            }
        };
        commit_write(
            cx,
            &resource,
            tx,
            Ok((written, crate::Outcome::done())),
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
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        // The version the form opened on: a form that posts none skips the comparison below,
        // and the write's own condition still refuses a save overtaken mid-transaction.
        let opened = parts
            .last(VERSION_FIELD)
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|error| topcoat::router::error::bad_request(error.to_string()))?;
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
        // Authoritative load inside the transaction observes the write snapshot.
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
        // Another save landed since the form opened: refuse this one rather than overwrite it.
        let stale = record
            .version()
            .is_some_and(|now| opened.is_some_and(|opened| opened != now));
        let Some(form) = form.filter(|_| errors.is_empty() && !stale) else {
            let chrome = FormChrome::edit(cx, &resource, &record);
            return rerender_invalid_form(
                cx,
                &resource,
                tx,
                if stale { chrome.stale() } else { chrome },
                &values,
                &errors,
                &carried,
            )
            .await;
        };
        let posted = Posted::new(form, named_fields(&resource, &named));
        // The version the write starts from: a model without one has no condition to fail on.
        let version = record.version();
        let written = match R::update_record(cx, record, posted, &mut tx).await {
            Ok(written) => written,
            Err(error) => {
                drop(tx);
                // A version condition means another save overtook this one; any other database
                // error may be a unique index refusing a value another write took.
                let was_overtaken = version.is_some() && overtaken(&error);
                let taken = if was_overtaken {
                    FieldErrors::new()
                } else {
                    taken_since(cx, &resource, &schema, &values, &stored, &error).await
                };
                if !was_overtaken && taken.is_empty() {
                    return Err(failed_write(cx, WRITE_UPDATE, error));
                }
                // The edit page renders on the record as it now stands.
                let record = load_editable(cx, &resource, &mut crate::db::db(cx)).await?;
                if !resource.can(cx, Ability::Update(&record)) {
                    return Err(forbidden().into());
                }
                let stale = was_overtaken || record.version() != version;
                let chrome = FormChrome::edit(cx, &resource, &record);
                return render_form_page(
                    cx,
                    &resource.form,
                    if stale { chrome.stale() } else { chrome },
                    &values,
                    &taken,
                    &carried,
                )
                .await;
            }
        };
        commit_write(
            cx,
            &resource,
            tx,
            Ok((written, crate::Outcome::done())),
            Committed::updated,
            "Updated",
            WRITE_UPDATE,
        )
        .await
    })
}

/// Whether a write failed on a Toasty condition, on a versioned model its `#[version]` one:
/// another save changed the record between the load and the write.
fn overtaken(error: &topcoat::Error) -> bool {
    error
        .downcast_ref::<toasty::Error>()
        .is_some_and(toasty::Error::is_condition_failed)
}

#[cfg(test)]
mod tests;
