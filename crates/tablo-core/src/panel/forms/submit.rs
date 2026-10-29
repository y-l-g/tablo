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
        write::{commit_write_then, failed_write},
    },
    common::{
        FilePosts, FormParts, blank, drop_client_typed_uploads, file_flags, gather_file_lists,
        parse_form_body, refuse_too_many_files, reject_unknown_form_keys, rerender_invalid_form,
        restore_pending_uploads,
    },
    related::{
        RelatedWrite, open_related, read_related, related_valid, render_related, write_related,
    },
    render::{Carried, FormChrome, VERSION_FIELD, form_action, render_form_page, seed_pairs},
    unique::{check_unique, taken_since},
};
use crate::{
    ModelColumns as _,
    db::{WriteTx, db},
    form::{FieldErrors, FormValues, Posted, RecordForm},
    panel::state::uploader,
    policy::Ability,
    resource::{Change, Mounted, Resource, WriteError},
    schema::Schema,
    topcoat_compat::async_page,
};

mod parse;

pub(super) use parse::{complete, named_fields, parse_form, recheck_relationships, refusal};

const WRITE_CREATE: &str = "tablo-failed-create";
const WRITE_UPDATE: &str = "tablo-failed-update";

pub(super) struct Submission {
    pub(super) schema: Arc<Schema>,
    pub(super) values: FormValues,
    pub(super) errors: FieldErrors,
    carried: Carried,
    /// Each file list the submission posts, which an edit resolves again against the record its
    /// transaction loads.
    lists: FilePosts,
    pub(super) named: HashSet<String>,
}

/// What a create submission's keys stand for when it does not post them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Unposted {
    /// What a browser's form leaves out: a key it renders a control for is posted, so one
    /// missing stays missing.
    Missing,
    /// An import row's: the header leaves the key out, so it takes the field's `default`, as the
    /// create page's untouched control posts it.
    Defaulted,
}

/// Stages a create/edit submission, completing unnamed keys from the edit advisory snapshot.
pub(super) async fn prepare_submission<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    parts: FormParts,
    advisory: Option<&R::Model>,
    unposted: Unposted,
) -> Result<Submission, topcoat::Error> {
    let schema = Arc::clone(&resource.form);
    // A field the user may not see or change renders no enabled control, so a browser posts
    // nothing for it: one posted anyway is refused rather than dropped.
    let mask = resource.form_mask(cx);
    let posted = parts.pairs.iter().map(|(name, _)| name.as_str());
    if let Some(key) = posted
        .chain(parts.file_part_names.iter().map(String::as_str))
        .find_map(|name| mask.writes(name))
    {
        return Err(topcoat::router::error::bad_request(format!(
            "`{key}` is not a field you may change"
        ))
        .into());
    }
    // Each repeater's rows, and each multiple choice's values, become its one key, which every
    // step below reads like any other.
    let (mut values, unclaimed) = schema
        .read_unclaimed(cx, &parts.pairs)
        .map_err(topcoat::router::error::bad_request)?;
    reject_unknown_form_keys(&schema, unclaimed)?;
    if unposted == Unposted::Defaulted {
        for (key, default) in schema.defaults().iter() {
            if values.get(key).is_none() {
                values.insert(key, default.clone());
            }
        }
    }
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
    // An edit completes a masked field from the stored record; a create takes its default.
    if !editing {
        values.extend(schema.defaults_of(mask.keys()));
    }
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
    // A control a condition disables posts nothing either, and the server reads it as a
    // disabled one: an edit keeps the stored value and a create takes the field's default.
    let disabled = schema.condition_disabled(&read);
    for key in &disabled {
        values.remove(key);
        files.remove(key);
        flags.remove(key);
    }
    if !editing {
        values.extend(schema.defaults_of(disabled.iter().map(String::as_str)));
    }
    // File fields take values only from file parts.
    drop_client_typed_uploads(&schema, &file_part_names, &mut values);
    // A list choosing more files than it holds stores none of them.
    let mut upload_errors = refuse_too_many_files(&schema, &mut files);
    let (stored_errors, mut singles) =
        crate::upload::store_uploads(uploader(cx), &schema, &files, &mut values).await;
    for error in stored_errors.iter() {
        if !upload_errors.contains_key(&error.key) {
            upload_errors.push(error.clone());
        }
    }
    singles.extend(restore_pending_uploads(cx, &schema, &flags, &mut values).await);
    let lists = gather_file_lists(cx, &schema, &flags, &file_part_names, &values).await;
    lists.resolve(&schema, &stored, &mut values, &mut upload_errors);
    let mut carried: Carried = lists.pending().collect();
    for name in singles {
        if let Some(path) = values.text(&name) {
            carried.insert((name.clone(), path.to_string()));
        }
    }
    // An empty file input keeps the stored value; `clear_<field>` clears it unless a file was
    // chosen.
    for (name, flags) in &flags {
        if !flags.clears() && blank(&values, name) && !blank(&stored, name) {
            values.remove(name);
        }
    }
    let named: HashSet<String> = values.keys().map(str::to_string).collect();
    complete(&schema, &mut values, &named, &stored);
    schema.keep_unedited(&mut values, &stored, &crate::time_zone(cx));
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
        lists,
        named,
    })
}

pub(crate) fn resource_create_post<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can_create(cx) {
            return Err(forbidden().into());
        }
        let mut parts = parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        let another = parts
            .last(crate::form::ANOTHER_FIELD)
            .is_some_and(super::common::truthy);
        let mut related = open_related(cx, &resource, None, &mut db(cx)?).await?;
        read_related(cx, &mut parts, &mut related).await?;
        let Submission {
            schema,
            values,
            mut errors,
            carried,
            ..
        } = prepare_submission(cx, &resource, parts, None, Unposted::Missing).await?;
        // Opens the transaction after validation so loaders never block on the held connection.
        let mut db = db(cx)?;
        let mut tx = WriteTx::begin(cx, &mut db)
            .await
            .map_err(crate::error::unavailable)?;
        // App-side unique check: Toasty exposes no unique-violation predicate (tablo#117). A
        // write the index refuses all the same re-probes through `taken_since`.
        errors.extend(
            check_unique(
                cx,
                &resource,
                &schema,
                &values,
                &FormValues::new(),
                &mut *tx,
            )
            .await?,
        );
        let form = parse_form(cx, &resource, &values, &mut errors)?;
        recheck_relationships(
            cx,
            &schema,
            &values,
            &FormValues::new(),
            &mut errors,
            &mut *tx,
        )
        .await;
        let Some(form) = form.filter(|_| errors.is_empty() && related_valid(&related)) else {
            return rerender_invalid_form(
                cx,
                &resource,
                tx,
                FormChrome::create(cx, &resource).with_related(render_related(cx, &related)),
                &values,
                &errors,
                &carried,
            )
            .await;
        };
        let written = match R::create_record(cx, form, &mut *tx).await {
            Ok(written) => written,
            Err(WriteError::Refused(errors)) => {
                drop(tx);
                let errors = refusal(&resource, "create_record", errors)?;
                return render_form_page(
                    cx,
                    &resource.form,
                    FormChrome::create(cx, &resource).with_related(render_related(cx, &related)),
                    &values,
                    &errors,
                    &carried,
                )
                .await;
            }
            Err(WriteError::Failed(error)) => {
                drop(tx);
                let taken =
                    taken_since(cx, &resource, &schema, &values, &FormValues::new(), &error).await;
                if taken.is_empty() {
                    return Err(failed_write(cx, WRITE_CREATE, error));
                }
                return render_form_page(
                    cx,
                    &resource.form,
                    FormChrome::create(cx, &resource).with_related(render_related(cx, &related)),
                    &values,
                    &taken,
                    &carried,
                )
                .await;
            }
        };
        // Each related form writes after the record it belongs to, whose key it holds.
        let after = match write_related(cx, &resource, &mut related, &written, &mut *tx).await {
            Ok(RelatedWrite::Done(after)) => after,
            Ok(written @ (RelatedWrite::Refused | RelatedWrite::Stale)) => {
                drop(tx);
                let chrome =
                    FormChrome::create(cx, &resource).with_related(render_related(cx, &related));
                return render_form_page(
                    cx,
                    &resource.form,
                    if matches!(written, RelatedWrite::Stale) {
                        chrome.stale()
                    } else {
                        chrome
                    },
                    &values,
                    &errors,
                    &carried,
                )
                .await;
            }
            Err(error) => {
                drop(tx);
                return Err(failed_write(cx, WRITE_CREATE, error));
            }
        };
        // Create and create another lands back on the create page, keeping its relationship
        // seeds and its `?return=`.
        let outcome = if crate::panel::url::picking(cx) {
            // A creatable choice's dialog reads the record off the page this lands on.
            let key = crate::toasty_compat::pk::pk_text(&written);
            crate::Outcome::done().redirect(crate::panel::url::picked_url(&resource.url, &key))
        } else if another {
            crate::Outcome::done().redirect(form_action(cx, &seed_pairs(cx, &resource)))
        } else {
            crate::Outcome::done()
        };
        commit_write_then(
            cx,
            &resource,
            tx,
            Ok((written, outcome)),
            Change::Create,
            crate::t!(cx, "tablo-form-created"),
            WRITE_CREATE,
            after,
        )
        .await
    })
}

/// Validates the edit submission and writes named fields, requiring both `View` and `Update`.
pub(crate) fn resource_edit_post<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let mut parts = parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        // The version the form opened on: a form that posts none skips the comparison below,
        // and the write's own condition still refuses a save overtaken mid-transaction.
        let opened = parts
            .last(VERSION_FIELD)
            .map(str::parse::<u64>)
            .transpose()
            .map_err(|error| topcoat::router::error::bad_request(error.to_string()))?;
        // Advisory load feeds validation; the authoritative load runs inside the transaction.
        let mut db0 = db(cx)?;
        let advisory = load_editable(cx, &resource, &mut db0).await?;
        if !resource.can(cx, Ability::Update(&advisory)) {
            return Err(forbidden().into());
        }
        let mut related = open_related(cx, &resource, Some(&advisory), &mut db0).await?;
        read_related(cx, &mut parts, &mut related).await?;
        let Submission {
            schema,
            mut values,
            mut errors,
            carried,
            lists,
            named,
        } = prepare_submission(cx, &resource, parts, Some(&advisory), Unposted::Missing).await?;
        // Authoritative load inside the transaction observes the write snapshot.
        let mut db = db(cx)?;
        let mut tx = WriteTx::begin(cx, &mut db)
            .await
            .map_err(crate::error::unavailable)?;
        let record = load_editable(cx, &resource, &mut *tx).await?;
        if !resource.can(cx, Ability::Update(&record)) {
            return Err(forbidden().into());
        }
        // Unnamed keys complete from this snapshot and are never written back.
        let stored = <R::Form as RecordForm>::hydrate(cx, &record);
        // A file another save added since the advisory load stays.
        lists.resolve(&schema, &stored, &mut values, &mut errors);
        complete(&schema, &mut values, &named, &stored);
        schema
            .keep_unseen_links(cx, &mut values, &stored, &mut *tx)
            .await;
        errors.extend(check_unique(cx, &resource, &schema, &values, &stored, &mut *tx).await?);
        let form = parse_form(cx, &resource, &values, &mut errors)?;
        recheck_relationships(cx, &schema, &values, &stored, &mut errors, &mut *tx).await;
        // Another save landed since the form opened: refuse this one rather than overwrite it.
        let stale = record
            .version()
            .is_some_and(|now| opened.is_some_and(|opened| opened != now));
        let Some(form) = form.filter(|_| errors.is_empty() && related_valid(&related) && !stale)
        else {
            let chrome =
                FormChrome::edit(cx, &resource, &record).with_related(render_related(cx, &related));
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
        let before = record.clone();
        let written = match R::update_record(cx, record, posted, &mut *tx).await {
            Ok(written) => written,
            Err(WriteError::Refused(errors)) => {
                drop(tx);
                let errors = refusal(&resource, "update_record", errors)?;
                // The rollback leaves the record as the transaction loaded it.
                return render_form_page(
                    cx,
                    &resource.form,
                    FormChrome::edit(cx, &resource, &before)
                        .with_related(render_related(cx, &related)),
                    &values,
                    &errors,
                    &carried,
                )
                .await;
            }
            Err(WriteError::Failed(error)) => {
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
                let record = load_editable(cx, &resource, &mut crate::db::db(cx)?).await?;
                if !resource.can(cx, Ability::Update(&record)) {
                    return Err(forbidden().into());
                }
                let stale = was_overtaken || record.version() != version;
                let chrome = FormChrome::edit(cx, &resource, &record)
                    .with_related(render_related(cx, &related));
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
        let after = match write_related(cx, &resource, &mut related, &written, &mut *tx).await {
            Ok(RelatedWrite::Done(after)) => after,
            Ok(written @ (RelatedWrite::Refused | RelatedWrite::Stale)) => {
                drop(tx);
                let chrome = FormChrome::edit(cx, &resource, &before)
                    .with_related(render_related(cx, &related));
                return render_form_page(
                    cx,
                    &resource.form,
                    if matches!(written, RelatedWrite::Stale) {
                        chrome.stale()
                    } else {
                        chrome
                    },
                    &values,
                    &errors,
                    &carried,
                )
                .await;
            }
            Err(error) => {
                drop(tx);
                return Err(failed_write(cx, WRITE_UPDATE, error));
            }
        };
        commit_write_then(
            cx,
            &resource,
            tx,
            Ok((written, crate::Outcome::done())),
            |after| Change::Update { before, after },
            crate::t!(cx, "tablo-form-updated"),
            WRITE_UPDATE,
            after,
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
