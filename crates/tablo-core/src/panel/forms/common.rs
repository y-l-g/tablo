//! Helpers the create/edit pipelines share.

use std::collections::{HashMap, HashSet};

use topcoat::{
    Result,
    context::Cx,
    router::{Body, error::forbidden},
    view::{BoxView, HoistView, internal::ThenView},
};

use super::{
    super::{actions::load_viewable, gate::gate},
    render::render_form_page,
};
use crate::{
    db::db,
    form::{FieldErrors, RecordForm},
    resource::Resource,
};

/// A decoded form body: the text values plus any file parts.
pub(crate) struct FormParts {
    pub(crate) values: HashMap<String, String>,
    /// File parts by field name, staged for the installed
    /// [`Uploader`](crate::Uploader) — empty when none is installed, because
    /// then the bytes could only be dropped and today's drain-and-discard
    /// is what keeps a large upload off the heap.
    pub(crate) files: HashMap<String, crate::upload::StagedUpload>,
    /// Field names that arrived as a multipart part carrying a `filename`
    /// (chosen or empty). Only these may set a file field's value: a
    /// text part or a url-encoded pair under the same name is client-typed, not
    /// an upload.
    pub(crate) file_part_names: HashSet<String>,
}

/// Max form/multipart body accepted: 10 MiB. It bounds the whole
/// multipart stream, file bytes included — whether they are discarded or
/// buffered for an installed [`Uploader`](crate::Uploader).
pub(crate) const MAX_FORM_BYTES: usize = 10 * 1024 * 1024;

/// Reject POST keys no declared Schema input owns (GH #89 mass-assignment
/// allow-list). `csrf_token` is a handler key, not a field, so it is filtered
/// before the check, as are `clear_<field>` flags for declared file
/// fields (explicit-clear convention — `truthy`); absent keys are fine (an
/// edit completes them from the stored record), unknown keys are a 400 — accepting
/// `role`/`tenant_id` smuggling would let a generic record fn iterating
/// `values` promote them to client-controlled writes.
pub(super) fn reject_unknown_form_keys(
    schema: &crate::schema::Schema,
    values: &HashMap<String, String>,
) -> Result<(), topcoat::Error> {
    // One transport-key vocabulary: the same `strip_transport_keys`
    // the record fns benefit from defines which keys the framework owns, so
    // the allow-list and the strip cannot drift apart.
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

/// The one boolean-vocabulary check for framework form flags:
/// `confirm=1|true`, `clear_<field>=1|true`. One vocabulary, not a per-handler
/// set.
pub(crate) fn truthy(v: &str) -> bool {
    v == "1" || v == "true"
}

/// Strip framework transport keys from the submitted values before any
/// record fn sees them: `csrf_token`, the `clear_<field>` flags and
/// the `keep_<field>` candidates a re-rendered form carries are
/// handler keys, not writable fields — a generic `Resource` impl iterating
/// `values` (the exact threat model in the `unknown_keys` docs) must not
/// receive them as writes. The framework strips once here, not per-app
/// convention.
pub(super) fn strip_transport_keys(
    schema: &crate::schema::Schema,
    values: &mut HashMap<String, String>,
) {
    let declared: HashSet<&str> = schema.fields().map(crate::schema::Field::name).collect();
    // A schema field literally named `csrf_token` (or `clear_<upload>`) is a
    // misconfiguration that would silently swallow its own value here — the
    // declared-name check keeps such a field's value flowing (the collision
    // is a build-time bug, not a transport key).
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

/// Drop any value a declared file field received from something other than a
/// file part. The field's value is the uploader's answer, the stored
/// value (edit backfill), or empty (clear) — never text the client typed, which
/// would reach the record and render as the file's link.
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

/// Re-use the upload a re-rendered form carried.
///
/// A re-rendered form posts each carried upload's path back under
/// `keep_<field>`, because the browser's file input is empty on the next
/// attempt. The candidate is used only when the installed uploader still holds
/// the path ([`crate::upload::holds`]): a client-typed value is never stored,
/// which is the GH #277 rule the carry must not re-open. Without an installed
/// uploader nothing can vouch for a path, so nothing is restored.
///
/// A field that carried a file of its own in this submission, or one the user
/// cleared, keeps its own answer. Returns the field names whose value is an
/// upload, so the caller can carry them through another re-render.
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

/// Shared create/edit POST error tail: re-render the form with inline
/// errors. Takes the open framework transaction by value and drops it before
/// rendering — the re-rendered form reloads relationship options on
/// its own handle, which would block on the pool while the tx holds it —
/// so the drop is enforced here rather than trusted at each call site.
//
// The public link rides through to the re-rendered form for the same reason
// as above.
#[allow(clippy::too_many_arguments)]
pub(super) async fn rerender_invalid_form<'a, R: Resource>(
    cx: &'a Cx,
    tx: toasty::Transaction<'_>,
    title: String,
    submit_label: &'static str,
    values: &HashMap<String, String>,
    errors: &FieldErrors,
    carried: &HashSet<String>,
    public_url: Option<String>,
) -> Result<BoxView<'a>> {
    drop(tx);
    render_form_page::<R>(cx, title, submit_label, values, errors, carried, public_url).await
}

/// Edit page GET — hydrates the form from the record the tenant-scoped
/// load returned.
pub(crate) fn resource_edit<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    Box::pin(HoistView::new(ThenView::new(async move {
        gate::<R>(cx)?;
        let mut db = db(cx);
        let record = load_viewable::<R>(cx, &mut db).await?;
        if !R::can_update(cx, &record) {
            return Err(forbidden().into());
        }
        crate::csrf::ensure_token(cx);
        let values = <R::Form as RecordForm>::hydrate(cx, &record);
        let public = R::public_url(cx, &record);
        let html = render_form_page::<R>(
            cx,
            format!("Edit {}", R::label()),
            "Save",
            &values,
            &FieldErrors::new(),
            &HashSet::new(),
            public,
        )
        .await?;
        Ok(html)
    })))
}
#[cfg(test)]
mod tests;
