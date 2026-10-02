//! Names where uploaded bytes go: a file field binds a `String` column and the
//! app's [`Uploader`] decides what path the record stores.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
};

use topcoat::context::Cx;

use crate::{form::FieldErrors, panel::state::current, schema::Schema};

/// Stores one uploaded file and names the value a record stores; installed once
/// per panel with [`Panel::uploads`](crate::Panel::uploads).
///
/// `filename` arrives sanitized to a non-empty basename without directories or
/// control characters; `bytes` are bounded by the 10 MiB form-body cap. The
/// framework stores the returned path verbatim.
pub trait Uploader: Send + Sync + 'static {
    /// Stores `bytes` and returns the value to store; the `Err` string renders
    /// inside the field's inline error, so it must be user-actionable.
    fn store(
        &self,
        filename: &str,
        bytes: &[u8],
    ) -> impl Future<Output = Result<String, String>> + Send;

    /// Answers `true` only for a path this store returned and still resolves
    /// inside its own root; the default answers `false`.
    fn holds(&self, path: &str) -> impl Future<Output = bool> + Send {
        async move {
            let _ = path;
            false
        }
    }
}

/// A submitted file part, staged only when an [`Uploader`] is installed.
#[derive(Debug, Clone)]
pub(crate) struct StagedUpload {
    pub(crate) filename: String,
    pub(crate) bytes: Vec<u8>,
}

/// The one uploader a panel was mounted with, in its `PanelState`.
pub(crate) struct InstalledUploader(Box<dyn DynUploader + Send + Sync>);

impl InstalledUploader {
    pub(crate) fn new(uploader: impl Uploader) -> Self {
        Self(Box::new(uploader))
    }
}

/// Boxed future of [`DynUploader::store`].
type StoreFuture<'a> = Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;

/// Boxed future of [`DynUploader::holds`].
type HoldFuture<'a> = Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

/// Dyn-compatible view of [`Uploader`].
pub(crate) trait DynUploader: Send + Sync {
    fn store<'a>(&'a self, filename: &'a str, bytes: &'a [u8]) -> StoreFuture<'a>;
    fn holds<'a>(&'a self, path: &'a str) -> HoldFuture<'a>;
}

impl<U: Uploader> DynUploader for U {
    fn store<'a>(&'a self, filename: &'a str, bytes: &'a [u8]) -> StoreFuture<'a> {
        Box::pin(Uploader::store(self, filename, bytes))
    }

    fn holds<'a>(&'a self, path: &'a str) -> HoldFuture<'a> {
        Box::pin(Uploader::holds(self, path))
    }
}

/// Whether this panel has an uploader installed.
pub(crate) fn installed(cx: &Cx) -> bool {
    installed_uploader(cx).is_some()
}

type DynUploaderRef<'a> = &'a (dyn DynUploader + Send + Sync);

fn installed_uploader<'a>(cx: &'a Cx) -> Option<DynUploaderRef<'a>> {
    current(cx)
        .and_then(|panel| panel.uploads.as_ref())
        .map(|installed| &*installed.0)
}

/// Whether the installed uploader still holds `path`; `false` without one.
pub(crate) async fn holds(cx: &Cx, path: &str) -> bool {
    match installed_uploader(cx) {
        Some(uploader) => uploader.holds(path).await,
        None => false,
    }
}

/// Runs the installed uploader over this form's file parts, returning inline
/// errors and the fields whose value is now the stored path. A failed store
/// becomes an inline field error and drops the submitted value; call outside
/// the write transaction.
pub(crate) async fn store_uploads(
    cx: &Cx,
    schema: &Schema,
    files: &HashMap<String, StagedUpload>,
    values: &mut HashMap<String, String>,
) -> (FieldErrors, HashSet<String>) {
    let Some(uploader) = installed_uploader(cx) else {
        return (FieldErrors::new(), HashSet::new());
    };
    let mut errors = FieldErrors::new();
    let mut stored = HashSet::new();
    // Declared uploads only: a file part the schema does not declare is not a
    // field this form may write (the unknown-key allow-list answers for it).
    for field in schema.fields().filter(|field| field.is_file()) {
        let name = field.name().to_string();
        let Some(staged) = files.get(&name) else {
            continue;
        };
        match uploader.store(&staged.filename, &staged.bytes).await {
            Ok(path) => {
                values.insert(name.clone(), path);
                stored.insert(name);
            }
            Err(reason) => {
                values.remove(&name);
                errors.add(
                    name,
                    format!("{} could not be uploaded: {reason}", field.label_str()),
                );
            }
        }
    }
    (errors, stored)
}
