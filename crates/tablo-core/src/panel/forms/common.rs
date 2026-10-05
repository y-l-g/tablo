//! Helpers the create/edit pipelines share.

use std::collections::{HashMap, HashSet};

use topcoat::{
    Result,
    context::Cx,
    router::{Body, error::forbidden},
    view::BoxView,
};

use super::{
    super::{actions::load_viewable, gate::gate},
    render::{FormChrome, render_form_page},
};
use crate::{
    db::db,
    form::{FieldErrors, RecordForm},
    policy::Ability,
    resource::{Mounted, Resource},
    topcoat_compat::async_page,
};

/// A decoded form body: the text values plus any file parts.
pub(crate) struct FormParts {
    pub(crate) values: HashMap<String, String>,
    /// Holds staged bytes only when an uploader is installed.
    pub(crate) files: HashMap<String, crate::upload::StagedUpload>,
    /// Names arriving as multipart parts carrying a `filename`; only these may set a file field's
    /// value.
    pub(crate) file_part_names: HashSet<String>,
}

/// Caps the whole multipart stream at 10 MiB, file bytes included.
pub(crate) const MAX_FORM_BYTES: usize = 10 * 1024 * 1024;

/// Rejects POST keys no declared schema input owns, returning 400 for unknown fields.
pub(super) fn reject_unknown_form_keys(
    schema: &crate::schema::Schema,
    values: &HashMap<String, String>,
) -> Result<(), topcoat::Error> {
    let mut filtered = values.clone();
    strip_transport_keys(schema, &mut filtered);
    let unknown = schema.unknown_keys(&filtered);
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

/// Strips `csrf_token`, `clear_<field>`, and `keep_<field>` keys before any record function sees
/// them.
pub(super) fn strip_transport_keys(
    schema: &crate::schema::Schema,
    values: &mut HashMap<String, String>,
) {
    let declared: HashSet<&str> = schema.fields().map(crate::schema::Field::name).collect();
    // A declared field keeps its own transport-shaped value; only undeclared transport keys are
    // stripped.
    values.retain(|k, _| {
        if k == crate::csrf::FIELD_NAME {
            return declared.contains(k.as_str());
        }
        let field = k.strip_prefix("clear_").or_else(|| k.strip_prefix("keep_"));
        match field {
            Some(field) if schema.fields().any(|f| f.is_file() && f.name() == field) => {
                declared.contains(k.as_str())
            }
            _ => true,
        }
    });
}

/// Drops file-field values that did not arrive as file parts.
pub(super) fn drop_client_typed_uploads(
    schema: &crate::schema::Schema,
    file_part_names: &HashSet<String>,
    values: &mut HashMap<String, String>,
) {
    for field in schema.fields().filter(|field| field.is_file()) {
        if !file_part_names.contains(field.name()) {
            values.remove(field.name());
        }
    }
}

/// Restores `keep_<field>` paths only when the installed uploader still holds them, returning
/// restored field names.
pub(super) async fn restore_pending_uploads(
    cx: &Cx,
    schema: &crate::schema::Schema,
    values: &mut HashMap<String, String>,
) -> HashSet<String> {
    let mut restored = HashSet::new();
    for field in schema.fields().filter(|field| field.is_file()) {
        let name = field.name();
        let empty = values
            .get(name)
            .map(|value| value.trim().is_empty())
            .unwrap_or(true);
        let cleared = values
            .get(&format!("clear_{name}"))
            .is_some_and(|value| truthy(value));
        if !empty || cleared {
            continue;
        }
        let Some(candidate) = values.get(&format!("keep_{name}")) else {
            continue;
        };
        let candidate = candidate.trim().to_string();
        if candidate.is_empty() || !crate::upload::holds(cx, &candidate).await {
            continue;
        }
        values.insert(name.to_string(), candidate);
        restored.insert(name.to_string());
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
    values: &HashMap<String, String>,
    errors: &FieldErrors,
    carried: &HashSet<String>,
) -> Result<BoxView<'a>> {
    drop(tx);
    render_form_page(cx, resource, chrome, values, errors, carried).await
}

/// Renders the edit form hydrated from the tenant-scoped record.
pub(crate) fn resource_edit<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let mut db = db(cx);
        let record = load_viewable(cx, &resource, &mut db).await?;
        if !resource.can(cx, Ability::Update(&record)) {
            return Err(forbidden().into());
        }
        crate::csrf::ensure_token(cx);
        let values = <R::Form as RecordForm>::hydrate(cx, &record);
        let html = render_form_page(
            cx,
            &resource,
            FormChrome::edit(cx, &resource, &record),
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
