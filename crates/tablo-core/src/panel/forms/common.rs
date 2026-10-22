//! Helpers the create/edit pipelines share.

use std::collections::{HashMap, HashSet};

use topcoat::{
    Result,
    context::Cx,
    router::{Body, error::forbidden},
    view::BoxView,
};

use super::{
    super::{actions::load_editable, gate::gate, relations::render_relations},
    render::{FormChrome, render_form_page},
};
use crate::{
    db::{WriteTx, db},
    form::{FieldErrors, FormValues, RecordForm},
    panel::state::uploader,
    policy::Ability,
    resource::{Mounted, Resource},
    topcoat_compat::async_page,
    upload::Capture,
};
pub(crate) use crate::{schema::MAX_FORM_BYTES, upload::FormParts};

/// Parses a panel form's body, staging each chosen file the panel's uploader can store.
pub(crate) async fn parse_form_body(cx: &Cx, body: Body) -> Result<FormParts, topcoat::Error> {
    let capture = if uploader(cx).is_some() {
        Capture::Persistable
    } else {
        Capture::None
    };
    crate::upload::parse_body(cx, body, capture).await
}

/// Rejects the posted keys no field claimed with 400, but the ones the form carries for itself:
/// the CSRF token, the edit form's record version, and each file field's `keep_<field>` and
/// `clear_<field>`.
pub(super) fn reject_unknown_form_keys(
    schema: &crate::schema::Schema,
    unclaimed: Vec<String>,
) -> Result<(), topcoat::Error> {
    let unknown: Vec<String> = unclaimed
        .into_iter()
        .filter(|key| {
            let file = key
                .strip_prefix("clear_")
                .or_else(|| key.strip_prefix("keep_"));
            key != crate::csrf::FIELD_NAME
                && key != super::render::VERSION_FIELD
                && !file
                    .is_some_and(|file| schema.fields().any(|f| f.is_file() && f.name() == file))
        })
        .collect();
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(topcoat::router::error::bad_request(format!(
            "unknown field(s): {}",
            unknown.join(", ")
        ))
        .into())
    }
}

/// Reports whether a `confirm` or `clear_<field>` flag value counts as true.
pub(crate) fn truthy(v: &str) -> bool {
    v == "1" || v == "true"
}

/// What a file field's control posts beside its file part.
#[derive(Debug, Default)]
pub(super) struct FileFlags {
    /// `keep_<field>`: the stored path a refused submission carried over.
    pub(super) keep: Option<String>,
    /// `clear_<field>`: whether the user cleared the stored file.
    pub(super) clear: bool,
}

/// Each file field's [`FileFlags`], from the last value `parts` posts under each flag's key.
pub(super) fn file_flags(
    schema: &crate::schema::Schema,
    parts: &FormParts,
) -> HashMap<String, FileFlags> {
    schema
        .fields()
        .filter(|field| field.is_file())
        .map(|field| {
            let name = field.name();
            let flags = FileFlags {
                keep: parts.last(&format!("keep_{name}")).map(str::to_string),
                clear: parts.last(&format!("clear_{name}")).is_some_and(truthy),
            };
            (name.to_string(), flags)
        })
        .collect()
}

/// Drops file-field values that did not arrive as file parts.
pub(super) fn drop_client_typed_uploads(
    schema: &crate::schema::Schema,
    file_part_names: &HashSet<String>,
    values: &mut FormValues,
) {
    for field in schema.fields().filter(|field| field.is_file()) {
        if !file_part_names.contains(field.name()) {
            values.remove(field.name());
        }
    }
}

/// Whether `values` holds no text under `key`, or a blank one.
pub(super) fn blank(values: &FormValues, key: &str) -> bool {
    values.text(key).is_none_or(|value| value.trim().is_empty())
}

/// Restores `keep_<field>` paths only when the panel signed them for the field and this browser,
/// the field's extensions take them and the installed uploader still holds them, returning
/// restored field names.
pub(super) async fn restore_pending_uploads(
    cx: &Cx,
    schema: &crate::schema::Schema,
    flags: &HashMap<String, FileFlags>,
    values: &mut FormValues,
) -> HashSet<String> {
    let mut restored = HashSet::new();
    for (name, flags) in flags {
        if !blank(values, name) || flags.clear {
            continue;
        }
        let Some(candidate) = flags
            .keep
            .as_deref()
            .and_then(|carried| crate::upload::carry::open(cx, name, carried.trim()))
        else {
            continue;
        };
        let taken = schema
            .fields()
            .find(|field| field.name() == name)
            .and_then(|field| field.as_file())
            .is_some_and(|file| file.takes_path(candidate));
        if !taken || candidate.is_empty() || !crate::upload::holds(uploader(cx), candidate).await {
            continue;
        }
        values.insert(name.clone(), candidate);
        restored.insert(name.clone());
    }
    restored
}

/// Drops the open transaction before re-rendering the form with inline errors, so option loaders
/// never block on the held connection.
pub(super) async fn rerender_invalid_form<'a, R: Resource>(
    cx: &'a Cx,
    resource: &Mounted<R>,
    tx: WriteTx<'_>,
    chrome: FormChrome<'a>,
    values: &FormValues,
    errors: &FieldErrors,
    carried: &HashSet<String>,
) -> Result<BoxView<'a>> {
    drop(tx);
    render_form_page(cx, &resource.form, chrome, values, errors, carried).await
}

/// Renders the edit form hydrated from the tenant-scoped record.
pub(crate) fn resource_edit<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let mut db = db(cx);
        let record = load_editable(cx, &resource, &mut db).await?;
        if !resource.can(cx, Ability::Update(&record)) {
            return Err(forbidden().into());
        }
        crate::csrf::ensure_token(cx);
        let values = <R::Form as RecordForm>::hydrate(cx, &record);
        // A table's rerun keeps the fields the reader edited, but drops repeater rows and fetched
        // options a script added (tablo#515). A refused save renders none: a rerun would drop
        // its errors.
        let relations = render_relations(cx, &resource, &record);
        let html = render_form_page(
            cx,
            &resource.form,
            FormChrome::edit(cx, &resource, &record).with_relations(relations),
            &values,
            &FieldErrors::new(),
            &HashSet::new(),
        )
        .await?;
        Ok(html)
    })
}
#[cfg(test)]
mod tests;
