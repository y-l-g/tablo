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
    db::db,
    form::{FieldErrors, FormValues, RecordForm},
    policy::Ability,
    resource::{Mounted, Resource},
    topcoat_compat::async_page,
};

/// A decoded form body: its text pairs plus any file parts.
pub(crate) struct FormParts {
    /// Every text value, and each file part's sanitized filename, in the order the body posts
    /// them.
    pub(crate) pairs: Vec<(String, String)>,
    /// Holds staged bytes only when an uploader is installed.
    pub(crate) files: HashMap<String, crate::upload::StagedUpload>,
    /// Names arriving as multipart parts carrying a `filename`; only these may set a file field's
    /// value.
    pub(crate) file_part_names: HashSet<String>,
}

impl FormParts {
    /// The last value the body posts under `key`.
    pub(crate) fn last(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .rev()
            .find(|(posted, _)| posted == key)
            .map(|(_, value)| value.as_str())
    }
}

/// Caps the whole multipart stream at 10 MiB, file bytes included.
pub(crate) const MAX_FORM_BYTES: usize = 10 * 1024 * 1024;

/// Rejects the posted keys no field claimed with 400, but the ones the form carries for itself:
/// the CSRF token, and each file field's `keep_<field>` and `clear_<field>`.
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

/// Restores `keep_<field>` paths only when the installed uploader still holds them, returning
/// restored field names.
pub(super) async fn restore_pending_uploads(
    cx: &Cx,
    flags: &HashMap<String, FileFlags>,
    values: &mut FormValues,
) -> HashSet<String> {
    let mut restored = HashSet::new();
    for (name, flags) in flags {
        if !blank(values, name) || flags.clear {
            continue;
        }
        let Some(candidate) = flags.keep.as_deref().map(str::trim) else {
            continue;
        };
        if candidate.is_empty() || !crate::upload::holds(cx, candidate).await {
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
    tx: toasty::Transaction<'_>,
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
