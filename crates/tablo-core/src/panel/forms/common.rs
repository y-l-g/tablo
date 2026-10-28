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
    render::{Carried, FormChrome, render_form_page},
};
pub(crate) use crate::upload::FormParts;
use crate::{
    db::{WriteTx, db},
    form::{FieldErrors, FormValue, FormValues, RecordForm},
    panel::state::uploader,
    policy::Ability,
    resource::{Mounted, Resource},
    topcoat_compat::async_page,
    upload::Capture,
};

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
/// the CSRF token, the edit form's record version, the create form's create-another button, and
/// each file field's `keep_<field>` and `clear_<field>`.
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
                && key != crate::form::ANOTHER_FIELD
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
    /// Every `keep_<field>` value, in the order the body posts them: a file field's carried
    /// upload, or each row a file list keeps.
    pub(super) keep: Vec<String>,
    /// Every `clear_<field>` value: a file field's flag clearing its stored file, or each path a
    /// file list drops.
    pub(super) clear: Vec<String>,
}

impl FileFlags {
    /// Whether a file field's last `clear_<field>` flag clears its stored file.
    pub(super) fn clears(&self) -> bool {
        self.clear.last().is_some_and(|flag| truthy(flag))
    }
}

/// Each file field's [`FileFlags`], from the values `parts` posts under each flag's key.
pub(super) fn file_flags(
    schema: &crate::schema::Schema,
    parts: &FormParts,
) -> HashMap<String, FileFlags> {
    schema
        .fields()
        .filter(|field| field.is_file())
        .map(|field| {
            let name = field.name();
            let posted = |key: String| -> Vec<String> {
                parts
                    .pairs
                    .iter()
                    .filter(|(posted, _)| *posted == key)
                    .map(|(_, value)| value.clone())
                    .collect()
            };
            let flags = FileFlags {
                keep: posted(format!("keep_{name}")),
                clear: posted(format!("clear_{name}")),
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
        let Some(file) = schema
            .fields()
            .find(|field| field.name() == name)
            .and_then(|field| field.as_file())
            .filter(|file| !file.multiple)
        else {
            continue;
        };
        if !blank(values, name) || flags.clears() {
            continue;
        }
        let Some(candidate) = flags
            .keep
            .last()
            .and_then(|carried| crate::upload::carry::open(cx, name, carried.trim()))
        else {
            continue;
        };
        let taken = file.takes_path(candidate);
        if !taken || candidate.is_empty() || !crate::upload::holds(uploader(cx), candidate).await {
            continue;
        }
        values.insert(name.clone(), candidate);
        restored.insert(name.clone());
    }
    restored
}

/// What a submission posts for one file list, resolved against the stored list by
/// [`FilePosts::resolve`].
#[derive(Debug)]
pub(super) struct FilePost {
    name: String,
    /// Each row's path, in the order the rows post them: the order the list keeps.
    order: Vec<String>,
    /// The refused submission's uploads the panel sealed for the field and this browser, that the
    /// field's extensions take and the installed uploader still holds.
    carried: Vec<String>,
    /// The paths the remove checkboxes drop.
    clear: HashSet<String>,
    /// The files this submission stored, in the order it posts them.
    added: Vec<String>,
}

impl FilePost {
    /// The list the submission leaves when the record stores `held`: the rows it posts, in its
    /// order, each a stored path or a carried upload; then each stored path no row posts; then the
    /// carried uploads no row posts; then the files it stored. Only a remove checkbox drops a
    /// stored path, so a row a script or a stale form leaves out, or another save added, stays.
    fn resolve(&self, held: &[String]) -> Vec<String> {
        let known: HashSet<&str> = held
            .iter()
            .chain(&self.carried)
            .map(String::as_str)
            .collect();
        let mut seen = HashSet::new();
        let mut paths = Vec::new();
        let posted = self
            .order
            .iter()
            .filter(|path| known.contains(path.as_str()));
        for path in posted.chain(held).chain(&self.carried) {
            if !self.clear.contains(path) && seen.insert(path.as_str()) {
                paths.push(path.clone());
            }
        }
        paths.extend(self.added.iter().cloned());
        paths
    }
}

/// Every file list a submission posts.
#[derive(Debug, Default)]
pub(super) struct FilePosts(Vec<FilePost>);

impl FilePosts {
    /// Sets each posted list in `values` to what it leaves of the list `stored` holds, adding a
    /// refusal to `errors` for one holding more than its field's `max_files`, unless the field has
    /// one already.
    pub(super) fn resolve(
        &self,
        schema: &crate::schema::Schema,
        stored: &FormValues,
        values: &mut FormValues,
        errors: &mut FieldErrors,
    ) {
        for post in &self.0 {
            let paths = post.resolve(stored.list(&post.name).unwrap_or_default());
            let field = schema.fields().find(|field| field.name() == post.name);
            let refusal = field.and_then(|field| {
                let file = field.as_file()?;
                file.count_refusal(&field.label(), paths.len())
            });
            if let Some(refusal) = refusal.filter(|_| !errors.contains_key(&post.name)) {
                errors.add(post.name.clone(), refusal);
            }
            values.insert(&post.name, FormValue::List(paths));
        }
    }

    /// The uploads each list holds that no record stores yet, which a refused submission carries
    /// to the next one.
    pub(super) fn pending(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.0.iter().flat_map(|post| {
            let paths = post.carried.iter().chain(&post.added);
            paths
                .filter(|path| !post.clear.contains(*path))
                .map(|path| (post.name.clone(), path.clone()))
        })
    }
}

/// Refuses, before anything is stored, each file list choosing more files than its field's
/// `max_files`, dropping its staged files so none is written.
pub(super) fn refuse_too_many_files(
    schema: &crate::schema::Schema,
    files: &mut HashMap<String, Vec<crate::upload::StagedUpload>>,
) -> FieldErrors {
    let mut errors = FieldErrors::new();
    for field in schema.fields() {
        let Some(file) = field.as_file().filter(|file| file.multiple) else {
            continue;
        };
        let chosen = files.get(field.name()).map_or(0, Vec::len);
        if let Some(refusal) = file.count_refusal(&field.label(), chosen) {
            files.remove(field.name());
            errors.add(field.name(), refusal);
        }
    }
    errors
}

/// What each file list the submission posts asks of it, taking the files `values` holds for it
/// as the ones it just stored. A list posts its input whether or not a file is chosen, so one
/// posting no input, row nor remove checkbox, as a hidden or disabled field does, is left to the
/// stored value.
///
/// A row posts its bare path under `keep_<field>`, and a refused submission's upload is carried
/// beside it under a `keep_<field>` the panel sealed for the field and this browser.
pub(super) async fn gather_file_lists(
    cx: &Cx,
    schema: &crate::schema::Schema,
    flags: &HashMap<String, FileFlags>,
    file_part_names: &HashSet<String>,
    values: &FormValues,
) -> FilePosts {
    let mut posts = Vec::new();
    for field in schema.fields() {
        let Some(file) = field.as_file().filter(|file| file.multiple) else {
            continue;
        };
        let name = field.name();
        let Some(flags) = flags.get(name) else {
            continue;
        };
        if !file_part_names.contains(name) && flags.keep.is_empty() && flags.clear.is_empty() {
            continue;
        }
        let mut order = Vec::new();
        let mut carried = Vec::new();
        for kept in &flags.keep {
            let kept = kept.trim();
            match crate::upload::carry::open(cx, name, kept) {
                Some(path) => {
                    let held = !path.is_empty()
                        && file.takes_path(path)
                        && crate::upload::holds(uploader(cx), path).await;
                    if held {
                        carried.push(path.to_string());
                    }
                }
                None => order.push(kept.to_string()),
            }
        }
        posts.push(FilePost {
            name: name.to_string(),
            order,
            carried,
            clear: flags
                .clear
                .iter()
                .map(|path| path.trim().to_string())
                .collect(),
            added: values.list(name).unwrap_or_default().to_vec(),
        });
    }
    FilePosts(posts)
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
    carried: &Carried,
) -> Result<BoxView<'a>> {
    drop(tx);
    render_form_page(cx, &resource.form, chrome, values, errors, carried).await
}

/// Renders the edit form hydrated from the tenant-scoped record.
pub(crate) fn resource_edit<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        let mut db = db(cx)?;
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
            &Carried::new(),
        )
        .await?;
        Ok(html)
    })
}
#[cfg(test)]
mod tests;
