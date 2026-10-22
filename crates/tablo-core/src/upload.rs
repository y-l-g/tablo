//! Names where uploaded bytes go: a file field binds a `String` column and the
//! app's [`Uploader`] decides what path the record stores.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    path::PathBuf,
};

use percent_encoding::percent_decode_str;

use crate::{
    BoxFuture,
    form::{FieldErrors, FormValues},
    schema::Schema,
    topcoat_compat::href::encode_path_segment,
};

/// Stores one uploaded file and names the value a record stores; installed once
/// per panel with [`Panel::uploads`](crate::Panel::uploads).
///
/// `filename` arrives sanitized to a non-empty basename without directories or
/// control characters; `bytes` are bounded by the 10 MiB form-body cap. The
/// framework stores the returned path verbatim. A file a file field's
/// [`extensions`](crate::FileField::extensions) or [`max_bytes`](crate::FileField::max_bytes)
/// refuse never reaches `store`.
///
/// - `store` returns the value to save. An `Err(reason)` is shown to the user inline as `<Label>
///   could not be uploaded: <reason>`, so keep the reason free of paths and driver messages.
/// - What a file field stores comes from a file part only: `store`'s answer, the stored value on an
///   untouched edit, or nothing when cleared. A path typed into the form is never stored.
/// - Keeping a file across a failed submit. When a form re-renders with errors, the file input is
///   empty again. The form carries the path `store` just returned, and the panel reuses it on the
///   next submit only when `Uploader::holds(path)` answers `true`. The default answers `false`, so
///   the user uploads again. Answer `true` only for a path your store could have written and still
///   holds inside its own root; checking whether an arbitrary path exists would let a client pick
///   any file. A `DirUploader` answers `true` for a URL spelled as its `store` spells one, naming a
///   file in its directory.
/// - Editing. The edit form shows the stored file as a link and an empty file input. Leaving it
///   empty keeps the file; the "Remove the current file" checkbox (`clear_<field>`) empties the
///   field. The input is required only while nothing is stored, and clearing a required field fails
///   validation, so declare the record-form field `#[form(optional)]` when a record may lose its
///   file.
/// - Links. The stored value renders as a link, on the edit form and in a `FileColumn` on the
///   detail page, only when it is a root-relative path (`/uploads/a.png`, not `//host`) or an
///   `http(s)` URL; anything else renders as text.
/// - Serving. `Panel::serve_dir(path, dir)` serves a directory (`uploads_dir` calls it), and the
///   served files are public unless the path sits under the panel prefix, which the auth gate
///   covers.
pub trait Uploader: Send + Sync + 'static {
    /// Stores `bytes` and returns the value to store; the `Err` string renders
    /// inside the field's inline error, so it must be user-actionable.
    fn store(
        &self,
        filename: &str,
        bytes: &[u8],
    ) -> impl Future<Output = Result<String, String>> + Send;

    /// Answers `true` only for a path this store could have written and still resolves inside its
    /// own root; the default answers `false`.
    ///
    /// A file field carries a refused submission's pending upload back through a hidden
    /// `keep_<field>`, so this is what decides which paths a record may point at: a store that
    /// answers `true` for a file it never wrote lets a forged carry point the record at it.
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

/// Whether `name` is a basename [`DirUploader::store`] writes: the UUID it minted, then the `-`
/// and the sanitized basename beside it, or nothing when that basename was empty.
fn is_stored_name(name: &str) -> bool {
    let Some((uuid, rest)) = name.split_at_checked(36) else {
        return false;
    };
    uuid::Uuid::try_parse(uuid).is_ok_and(|parsed| parsed.to_string() == uuid)
        && (rest.is_empty() || (rest.len() > 1 && rest.starts_with('-')))
}

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

    /// Whether the directory holds the file a URL `store` returned names. The name `store` writes
    /// is the check, since the directory keeps no index of what it wrote: a file whose name it
    /// would not write is not held.
    async fn holds(&self, path: &str) -> bool {
        let Some(name) = self.file_name(path) else {
            return false;
        };
        if !is_stored_name(&name) {
            return false;
        }
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
/// assert_eq!(tablo_core::sanitize_filename("../../etc/passwd"), "passwd");
/// assert_eq!(
///     tablo_core::sanitize_filename("C:\\fakepath\\cover.png"),
///     "cover.png"
/// );
/// assert_eq!(tablo_core::sanitize_filename(".."), "");
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

/// Dyn-compatible view of [`Uploader`].
pub(crate) trait DynUploader: Send + Sync {
    fn store<'a>(
        &'a self,
        filename: &'a str,
        bytes: &'a [u8],
    ) -> BoxFuture<'a, Result<String, String>>;
    fn holds<'a>(&'a self, path: &'a str) -> BoxFuture<'a, bool>;
}

impl<U: Uploader> DynUploader for U {
    fn store<'a>(
        &'a self,
        filename: &'a str,
        bytes: &'a [u8],
    ) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(Uploader::store(self, filename, bytes))
    }

    fn holds<'a>(&'a self, path: &'a str) -> BoxFuture<'a, bool> {
        Box::pin(Uploader::holds(self, path))
    }
}

/// Whether `uploader`, a panel's installed one, still holds `path`; `false` without one.
pub(crate) async fn holds(uploader: Option<&InstalledUploader>, path: &str) -> bool {
    match uploader {
        Some(uploader) => uploader.0.holds(path).await,
        None => false,
    }
}

/// Runs `uploader`, a panel's installed one, over this form's file parts, returning inline
/// errors and the fields whose value is now the stored path. A file the field's extensions or
/// size refuse is not stored; it, or a failed store, becomes an inline field error and drops the
/// submitted value. Call outside the write transaction.
pub(crate) async fn store_uploads(
    uploader: Option<&InstalledUploader>,
    schema: &Schema,
    files: &HashMap<String, StagedUpload>,
    values: &mut FormValues,
) -> (FieldErrors, HashSet<String>) {
    let Some(InstalledUploader(uploader)) = uploader else {
        return (FieldErrors::new(), HashSet::new());
    };
    let mut errors = FieldErrors::new();
    let mut stored = HashSet::new();
    // Declared uploads only: a file part the schema does not declare is not a
    // field this form may write (the unknown-key allow-list answers for it).
    for field in schema.fields() {
        let Some(file) = field.as_file() else {
            continue;
        };
        let name = field.name().to_string();
        let Some(staged) = files.get(&name) else {
            continue;
        };
        // A file the field's rules refuse is never stored.
        let label = field.label_str();
        let filename = sanitize_filename(&staged.filename);
        let stored_path = match file.refusal(label, &filename, staged.bytes.len()) {
            Some(refusal) => Err(refusal),
            None => uploader
                .store(&staged.filename, &staged.bytes)
                .await
                .map_err(|reason| format!("{label} could not be uploaded: {reason}")),
        };
        match stored_path {
            Ok(path) => {
                values.insert(name.clone(), path);
                stored.insert(name);
            }
            Err(message) => {
                values.remove(&name);
                errors.add(name, message);
            }
        }
    }
    (errors, stored)
}

#[cfg(test)]
mod tests;
