//! The create/edit POST pipelines.

use std::collections::{HashMap, HashSet};

use topcoat::{
    Result,
    context::Cx,
    router::{Body, error::forbidden},
    view::{BoxView, HoistView, internal::ThenView},
};

use super::{
    super::{actions::find_by_key_narrowed, gate::gate},
    common::{
        FormParts, drop_client_typed_uploads, redirect_after_write, reject_unknown_form_keys,
        rerender_invalid_form, restore_pending_uploads, strip_transport_keys, truthy,
    },
    decode::parse_form_body,
    unique::check_unique,
};
use crate::{
    db::db,
    form::{FieldErrorKind, Posted, RecordForm},
    notification::notify_write_failure,
    resource::{Committed, Resource},
    schema::Schema,
};

/// Failure-toast wording for the create/update handlers: one place,
/// so the two paths cannot drift.
const WRITE_CREATE: &str = "create the record";
const WRITE_UPDATE: &str = "save the changes";

/// The staged submission both write handlers carry into their transaction: the
/// declared schema, the completed values, the validation errors so far, the
/// upload paths a re-render keeps, and the keys the submission named.
struct Submission {
    schema: Schema,
    values: HashMap<String, String>,
    errors: HashMap<String, Vec<String>>,
    carried: HashSet<String>,
    named: HashSet<String>,
}

/// Stage a create/edit submission: reject undeclared keys, take file values
/// only from file parts, store the uploads outside the transaction, restore the
/// paths a re-rendered form carried, strip the transport keys, record the keys
/// the submission names, complete the rest from `advisory`, and validate —
/// required and unique-free checks first, then the async relationship
/// existence check.
///
/// `advisory` is the edit path's pre-transaction snapshot. A create passes
/// `None`, so nothing is completed and an absent key validates as `""`.
async fn prepare_submission<R: Resource>(
    cx: &Cx,
    parts: FormParts,
    advisory: Option<&R::Model>,
) -> Result<Submission, topcoat::Error> {
    let schema = R::form(cx);
    reject_unknown_form_keys(&schema, &parts.values)?;
    let FormParts {
        mut values,
        files,
        file_part_names,
    } = parts;
    let stored = advisory
        .map(|advisory| <R::Form as RecordForm>::hydrate(cx, advisory))
        .unwrap_or_default();
    // A declared `FileUpload` takes its value only from a file part:
    // a text part or a url-encoded pair under the same name is client-typed,
    // not an upload, and would otherwise reach the record and render as the
    // file's link.
    drop_client_typed_uploads(&schema, &file_part_names, &mut values);
    // Uploaded bytes become stored paths before validation, and outside the
    // transaction: an upload is a side effect in another system, so a
    // rolled-back transaction must not have to undo it, and a store that
    // rejects the file must be able to answer inline.
    let (upload_errors, mut carried) =
        crate::upload::store_uploads(cx, &schema, &files, &mut values).await;
    // A form re-rendered after a failed submit carries the path its store just
    // answered; the uploader must still hold it. A restored field is
    // non-empty, so the untouched check below leaves it alone.
    carried.extend(restore_pending_uploads(cx, &schema, &mut values).await);
    // An untouched file input is not named: the edit form renders an empty file
    // input (browsers never pre-fill it), so an empty part means "keep", not
    // "clear". An explicit `clear_<field>=1` names it empty; a chosen file
    // still wins over the clear, because a replacement is not a removal.
    for name in schema.file_uploads().keys() {
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
    // Transport keys never reach the record fn; see `strip_transport_keys`.
    strip_transport_keys(&schema, &mut values);
    let named: HashSet<String> = values.keys().cloned().collect();
    complete(&schema, &mut values, &named, &stored);
    let mut errors = schema.validate_async(cx, &values).await;
    // A rejected upload owns its field's error slot: "required" would restate
    // the symptom (nothing was stored) and hide the reason.
    errors.extend(upload_errors);
    Ok(Submission {
        schema,
        values,
        errors,
        carried,
        named,
    })
}

/// Fill every declared key the submission did not name from `stored`, the
/// record's form projection, and drop one `stored` does not hold. On create
/// `stored` is empty and an unnamed key stays absent.
fn complete(
    schema: &Schema,
    values: &mut HashMap<String, String>,
    named: &HashSet<String>,
    stored: &HashMap<String, String>,
) {
    for name in schema.field_names() {
        if named.contains(&name) {
            continue;
        }
        match stored.get(&name) {
            Some(value) => values.insert(name, value.clone()),
            None => values.remove(&name),
        };
    }
}

/// Parse the completed values into the resource's form and run its
/// `validate_record`, adding each failure to `errors`.
///
/// A parse failure is added only to a key with no error yet, so a blank
/// required control shows the schema's message once. `validate_record` needs a
/// whole form, so it runs only when every field parsed.
///
/// # Errors
///
/// A `validate_record` error on a field `RecordForm::fields` binds to no key:
/// it cannot render, and the write must not proceed past it.
fn parse_form<R: Resource>(
    cx: &Cx,
    schema: &Schema,
    values: &HashMap<String, String>,
    errors: &mut HashMap<String, Vec<String>>,
) -> Result<Option<R::Form>, topcoat::Error> {
    // Typed fields parse their own spelling, not the browser's.
    let mut normalized = values.clone();
    schema.normalize_values(&mut normalized);
    match <R::Form as RecordForm>::parse(cx, &normalized) {
        Ok(form) => {
            let fields = <R::Form as RecordForm>::fields(cx);
            for (field, message) in R::validate_record(cx, &form).iter() {
                // A field's errors render under its first key: a scalar's own,
                // an embedded enum's discriminant.
                let Some(key) = fields
                    .iter()
                    .find(|claim| claim.field == *field)
                    .and_then(|claim| claim.keys.first())
                else {
                    // A rule refused a field the form lists no key for: there is
                    // nowhere to render it, and writing anyway would drop it.
                    return Err(std::io::Error::other(format!(
                        "validate_record refused {field:?}, which `{}::fields` binds to no key: \
                         {message}",
                        std::any::type_name::<R::Form>()
                    ))
                    .into());
                };
                errors.entry(key.clone()).or_default().push(message.clone());
            }
            Ok(Some(form))
        }
        Err(failures) => {
            let controls = schema.controls();
            for failure in failures {
                if errors.contains_key(&failure.key) {
                    continue;
                }
                let message = match failure.kind {
                    FieldErrorKind::Required => controls
                        .iter()
                        .find(|control| control.name == failure.key)
                        .and_then(|control| control.required_error.clone())
                        .unwrap_or(failure.message),
                    FieldErrorKind::Invalid => failure.message,
                };
                errors.insert(failure.key, vec![message]);
            }
            Ok(None)
        }
    }
}

/// The form fields with at least one key the submission named.
fn named_fields<F: RecordForm>(cx: &Cx, named: &HashSet<String>) -> Vec<F::Field> {
    F::fields(cx)
        .into_iter()
        .filter(|field| field.keys.iter().any(|key| named.contains(key)))
        .map(|field| field.field)
        .collect()
}

/// The shared write tail: commit the transaction,
/// run the after-commit hook on the row the record fn wrote, and redirect with
/// the success flash; a failed write or commit maps to the caller's operation
/// toast and the opaque error.
///
/// `committed` names the mutation, `note` the success flash, and `failure` the
/// toast.
async fn commit_write<'a, R: Resource>(
    cx: &'a Cx,
    tx: toasty::Transaction<'_>,
    written: Result<R::Model, topcoat::Error>,
    committed: impl FnOnce(R::Model) -> Committed<R::Model>,
    note: &'static str,
    failure: &'static str,
) -> Result<BoxView<'a>, topcoat::Error> {
    match written {
        Ok(record) => match tx.commit().await {
            Ok(()) => {
                // Post-commit, so the effect cannot survive a rollback
                // the tx is gone, so the hook may open its own
                // handle.
                crate::resource::run_after_commit::<R>(cx, committed(record)).await;
                Err(redirect_after_write::<R>(cx, note))
            }
            Err(error) => {
                notify_write_failure(cx, failure);
                Err(crate::db::unavailable(error))
            }
        },
        // A unique violation that slipped past the app-side check (a
        // concurrent write) surfaces as an error, not a string-matched inline
        // message: Toasty exposes no unique-violation predicate (upstream gap
        // #117), so the failure cannot be classified here. It is still not
        // echoed raw: the driver's text goes to the log through the
        // opaque mapping, and an app-authored hook error keeps its own.
        Err(error) => {
            notify_write_failure(cx, failure);
            Err(crate::db::hook_failure(error))
        }
    }
}

pub(crate) fn resource_create_post<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        if !R::can_create(cx) {
            return Err(forbidden().into());
        }
        let parts = parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, &parts.values)?;
        // A create has no stored value to keep, so it stages no advisory
        // snapshot: a rejected file leaves its field empty beside the reason.
        let Submission {
            schema,
            values,
            mut errors,
            carried,
            ..
        } = prepare_submission::<R>(cx, parts, None).await?;
        // Framework-owned transaction, opened only after validation so that
        // `validate_async` loaders still run before it opens (see `crate::db`
        // pool discipline). The unique check and the write observe one snapshot
        // and commit atomically; dropping `tx` without commit rolls back.
        let mut db = db(cx);
        let mut tx = db.transaction().await.map_err(crate::db::unavailable)?;
        // App-side unique check over every `unique()`-marked input — the only
        // error layer until toasty exposes a unique-violation predicate
        // (upstream gap #117; never string-match driver error messages).
        for (name, errs) in
            check_unique::<R>(cx, &schema, &values, &HashMap::new(), &mut tx).await?
        {
            errors.entry(name).or_default().extend(errs);
        }
        let form = parse_form::<R>(cx, &schema, &values, &mut errors)?;
        let Some(form) = form.filter(|_| errors.is_empty()) else {
            return rerender_invalid_form::<R>(
                cx,
                tx,
                format!("Create {}", R::navigation_label()),
                "Create",
                &values,
                &errors,
                &carried,
                None,
            )
            .await;
        };
        // The row the record fn returns is what `after_commit` names for this
        // write — the key is the database's to generate, so the row is the
        // only place the framework can learn it.
        let written = R::create_record(cx, form, &mut tx).await;
        commit_write::<R>(cx, tx, written, Committed::created, "Created", WRITE_CREATE).await
    })))
}

/// Edit page POST — validates, checks `can_view` + `can_update`, and writes
/// the fields the submission named.
///
/// Requires both `can_view` and `can_update` (matching GET, deny-by-default):
/// a view-denied but writable record must not be mutable by direct POST.
pub(crate) fn resource_edit_post<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        let parts = parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, &parts.values)?;
        let id = topcoat::router::path_param_segment(cx, "id").to_string();
        // Advisory load on a pooled handle: feeds the completion validation
        // reads. The body is already parsed and CSRF-verified, so the load
        // never runs for a forged POST. The authoritative load + policy check
        // happens inside the framework transaction; validation's
        // `validate_async` loaders run before it opens (see `crate::db` pool
        // discipline).
        let mut db0 = db(cx);
        let advisory = find_by_key_narrowed::<R>(cx, &id, &mut db0).await?;
        if !R::can_view(cx, &advisory) {
            return Err(forbidden().into());
        }
        if !R::can_update(cx, &advisory) {
            return Err(forbidden().into());
        }
        let Submission {
            schema,
            mut values,
            mut errors,
            carried,
            named,
        } = prepare_submission::<R>(cx, parts, Some(&advisory)).await?;
        // Authoritative load inside the framework transaction (#86):
        // policy is checked on this snapshot and the same record flows into
        // the write — never a silent re-load outside the checked snapshot.
        let mut db = db(cx);
        let mut tx = db.transaction().await.map_err(crate::db::unavailable)?;
        let record = find_by_key_narrowed::<R>(cx, &id, &mut tx).await?;
        if !R::can_view(cx, &record) {
            return Err(forbidden().into());
        }
        if !R::can_update(cx, &record) {
            return Err(forbidden().into());
        }
        // The unnamed keys come from this snapshot, not the advisory one: the
        // parsed form reads what the write sees, and no unnamed key is ever
        // written back.
        let stored = <R::Form as RecordForm>::hydrate(cx, &record);
        complete(&schema, &mut values, &named, &stored);
        for (name, errs) in check_unique::<R>(cx, &schema, &values, &stored, &mut tx).await? {
            errors.entry(name).or_default().extend(errs);
        }
        let form = parse_form::<R>(cx, &schema, &values, &mut errors)?;
        let Some(form) = form.filter(|_| errors.is_empty()) else {
            let public = R::public_url(cx, &record);
            return rerender_invalid_form::<R>(
                cx,
                tx,
                format!("Edit {}", R::navigation_label()),
                "Save",
                &values,
                &errors,
                &carried,
                public,
            )
            .await;
        };
        let posted = Posted::new(form, named_fields::<R::Form>(cx, &named));
        let written = R::update_record(cx, record, posted, &mut tx).await;
        commit_write::<R>(cx, tx, written, Committed::updated, "Updated", WRITE_UPDATE).await
    })))
}

#[cfg(test)]
mod tests;
