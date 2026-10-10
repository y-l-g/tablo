//! Names where uploaded bytes go: a file field binds a `String` column and the
//! app's [`Uploader`] decides what path the record stores.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    path::PathBuf,
    pin::Pin,
};

use percent_encoding::percent_decode_str;
use topcoat::context::Cx;

use crate::{
    form::FieldErrors, panel::state::current, schema::Schema,
    topcoat_compat::href::encode_path_segment,
};

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

/// Writes each upload into one directory as `{uuid}-{basename}` and stores its URL under
/// `url_prefix`; [`Panel::uploads_dir`](crate::Panel::uploads_dir) installs it and serves the
/// directory there.
///
/// ```no_run
/// # use tablo_core::{DirUploader, Panel};
/// Panel::new("admin")
///     .uploads(DirUploader::new("/uploads", "var/uploads"))
///     .serve_dir("/uploads/{*file}", "var/uploads");
/// ```
#[derive(Debug, Clone)]
pub struct DirUploader {
    url_prefix: String,
    dir: PathBuf,
}

/// The longest basename [`DirUploader`] keeps: 255 bytes, less the 36-byte UUID and its dash.
const MAX_STORED_BASENAME: usize = 255 - 37;

impl DirUploader {
    /// Writes into `dir`, naming each file by a URL under `url_prefix` (`"/uploads"`).
    pub fn new(url_prefix: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        let url_prefix = url_prefix.into().trim_end_matches('/').to_string();
        Self {
            url_prefix,
            dir: dir.into(),
        }
    }

    /// The file a stored URL names: one segment under the prefix, spelled as `store` spells it.
    fn file_name(&self, path: &str) -> Option<String> {
        let segment = path.strip_prefix(&self.url_prefix)?.strip_prefix('/')?;
        let name = percent_decode_str(segment).decode_utf8().ok()?;
        (!name.is_empty()
            && sanitize_filename(&name) == name
            && encode_path_segment(&name) == segment)
            .then(|| name.into_owned())
    }
}

impl Uploader for DirUploader {
    async fn store(&self, filename: &str, bytes: &[u8]) -> Result<String, String> {
        let basename = sanitize_filename(filename);
        let mut cut = basename.len().saturating_sub(MAX_STORED_BASENAME);
        while !basename.is_char_boundary(cut) {
            cut += 1;
        }
        let name = match &basename[cut..] {
            "" => uuid::Uuid::new_v4().to_string(),
            basename => format!("{}-{basename}", uuid::Uuid::new_v4()),
        };
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|_| "the upload directory is not writable".to_string())?;
        tokio::fs::write(self.dir.join(&name), bytes)
            .await
            .map_err(|_| "the upload could not be written".to_string())?;
        Ok(format!(
            "{}/{}",
            self.url_prefix,
            encode_path_segment(&name)
        ))
    }

    /// Whether the directory holds the file a URL `store` returned names.
    async fn holds(&self, path: &str) -> bool {
        let Some(name) = self.file_name(path) else {
            return false;
        };
        tokio::fs::metadata(self.dir.join(name))
            .await
            .is_ok_and(|metadata| metadata.is_file())
    }
}

/// Strips a client filename to a safe basename capped at 255 bytes, rejecting `.`, `..`, and
/// Windows reserved names to empty: what a file field's [`Uploader`] receives, for a page that
/// parses its own multipart body.
///
/// ```
/// assert_eq!(
///     tablo_core::upload::sanitize_filename("../../etc/passwd"),
///     "passwd"
/// );
/// assert_eq!(
///     tablo_core::upload::sanitize_filename("C:\\fakepath\\cover.png"),
///     "cover.png"
/// );
/// assert_eq!(tablo_core::upload::sanitize_filename(".."), "");
/// ```
pub fn sanitize_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or(raw).trim();
    let clean: String = base.chars().filter(|c| !c.is_control()).collect();
    let trimmed = clean.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed == "." || trimmed == ".." || is_windows_reserved_name(trimmed) {
        return String::new();
    }
    // Caps at 255 bytes, advancing the cut to a char boundary to avoid panicking.
    if trimmed.len() > 255 {
        let mut start = trimmed.len() - 255;
        while !trimmed.is_char_boundary(start) {
            start += 1;
        }
        trimmed[start..].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Reports whether a basename is a Windows reserved device name.
fn is_windows_reserved_name(name: &str) -> bool {
    let stem = match name.split_once('.') {
        Some((stem, _)) => stem,
        None => name,
    };
    let stem = stem.to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let Some(n) = stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
    else {
        return false;
    };
    n.parse::<u8>().is_ok_and(|n| (1..=9).contains(&n))
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

#[cfg(test)]
mod tests;
