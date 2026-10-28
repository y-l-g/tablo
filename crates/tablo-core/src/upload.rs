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
    form::{FieldErrors, FormValue, FormValues},
    schema::Schema,
    topcoat_compat::href::encode_path_segment,
};

mod body;
pub(crate) mod carry;
mod key;
mod pending;
pub(crate) mod signed;

pub(crate) use self::body::{Capture, FormParts, parse_body, parse_file_body};

/// Stores one uploaded file and names the value a record stores; installed once
/// per panel with [`Panel::uploads`](crate::Panel::uploads).
///
/// `filename` arrives sanitized to a non-empty basename without directories or
/// control characters; `bytes` are bounded by the panel's
/// [`Limits::body_bytes`](crate::Limits::body_bytes). The
/// framework stores the returned path verbatim. A file a file field's
/// [`extensions`](crate::FileField::extensions) or [`max_bytes`](crate::FileField::max_bytes)
/// refuse never reaches `store`.
///
/// - `store` returns the value to save. An `Err(reason)` is shown to the user inline as `<Label>
///   could not be uploaded: <reason>`, so keep the reason free of paths and driver messages.
/// - What a file field stores comes from a file part only: `store`'s answer, the stored value on an
///   untouched edit, or nothing when cleared. A path typed into the form is never stored.
/// - Keeping a file across a failed submit. When a form re-renders with errors, the file input is
///   empty again. The form carries the path `store` just returned, under a tag the panel signs for
///   that field and that browser's CSRF cookie, so a client cannot carry a path the field's rules
///   never passed. The panel reuses it on the next submit only when the tag holds and
///   `Uploader::holds(path)` answers `true`. The default answers `false`, so the user uploads
///   again. The tag's key derives from the router's cookie `Key`, else from one drawn per process,
///   so without a shared `Key` a carry reaches only the instance that minted it. Answer `true` only
///   for a path your store could have written and still holds inside its own root; checking whether
///   an arbitrary path exists would let a client pick any file. A `DirUploader` answers `true` for
///   a URL spelled as its `store` spells one, naming a file in its directory.
/// - Editing. The edit form shows the stored file as a link and an empty file input. Leaving it
///   empty keeps the file; the "Remove the current file" checkbox (`clear_<field>`) empties the
///   field. The input is required only while nothing is stored, and clearing a required field fails
///   validation, so declare the record-form field `#[form(optional)]` when a record may lose its
///   file. A [`Field::files`](crate::Field::files) list shows a row per stored file, each posting
///   its path back under `keep_<field>` to order the list, and a remove checkbox posting it under
///   `clear_<field>`, the only way a save drops a stored file; the rows keep the order they post
///   in, chosen files are stored after them, and a refused submission's uploads are carried beside
///   them as a single field's is.
/// - Links. The stored value renders as a link, on the edit form and in a `FileColumn` on the
///   detail page, only when it is a root-relative path (`/uploads/a.png`, not `//host`) or an
///   `http(s)` URL; anything else renders as text.
/// - Serving. `Panel::uploads_dir` serves its directory only at signed URLs that expire, which the
///   panel's pages render; `Panel::serve_dir(path, dir)` serves a directory publicly unless the
///   path sits under the panel prefix, which the auth gate covers.
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

    /// Confirms that the write about to commit stores `path`; the default does nothing.
    ///
    /// The panel calls it inside the write's transaction, before the commit, for each new path a
    /// file field's create or edit stores and [`holds`](Self::holds) answers `true` for. A store
    /// that deletes what nothing kept, as [`DirUploader::expire_pending`] does, keeps `path` here.
    /// An `Err(reason)` fails the write, so a file is never deleted under a committed record.
    ///
    /// It has no default: an uploader wrapping another forwards it, or the inner one deletes the
    /// files of saved records. A store that deletes nothing answers `Ok(())`.
    fn keep(&self, path: &str) -> impl Future<Output = Result<(), String>> + Send;

    /// Deletes the file at `path`, which a committed write no longer stores; the default keeps
    /// every file.
    ///
    /// The panel calls it after the write commits, once per path, and only for a path
    /// [`holds`](Self::holds) answers `true` for: a record's files when a delete removes its row
    /// (a soft delete keeps them until the force delete), and a file field's previous file when
    /// an edit clears or replaces it. An `Err(reason)` is logged; the write stands. A path two
    /// records share goes with the first that lets it go.
    fn delete(&self, path: &str) -> impl Future<Output = Result<(), String>> + Send {
        async move {
            let _ = path;
            Ok(())
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
    pending: Option<std::sync::Arc<pending::Pending>>,
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
            pending: None,
        }
    }

    /// Marks each file it stores pending until the panel's write that stores it commits, and
    /// deletes a pending file once it is `age` old: the upload of a submission that never saved,
    /// such as a refused form the user left. [`Panel::uploads_dir`](crate::Panel::uploads_dir)
    /// sets 24 hours.
    ///
    /// A store sweeps at most once an hour. Only the panel's record forms keep what they store:
    /// an app page that stores through this uploader and saves the path itself keeps it with
    /// [`Uploader::keep`], or uses an uploader without this setting.
    pub fn expire_pending(mut self, age: std::time::Duration) -> Self {
        self.pending = Some(std::sync::Arc::new(pending::Pending::new(age)));
        self
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
        if let Some(pending) = &self.pending {
            pending.sweep_if_due(&self.dir);
            pending
                .mark(&self.dir, &name)
                .await
                .map_err(|_| "the upload could not be written".to_string())?;
        }
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

    /// Drops the pending mark of the file a URL `store` returned names.
    async fn keep(&self, path: &str) -> Result<(), String> {
        match self.file_name(path).filter(|name| is_stored_name(name)) {
            Some(name) => pending::keep(&self.dir, &name)
                .await
                .map_err(|error| error.to_string()),
            None => Ok(()),
        }
    }

    /// Removes the file a URL `store` returned names, and its pending mark; a file already gone
    /// is no error.
    async fn delete(&self, path: &str) -> Result<(), String> {
        let Some(name) = self.file_name(path).filter(|name| is_stored_name(name)) else {
            return Ok(());
        };
        match tokio::fs::remove_file(self.dir.join(&name)).await {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(error.to_string());
            }
            _ => {}
        }
        pending::unmark(&self.dir, &name)
            .await
            .map_err(|error| error.to_string())
    }
}

/// Strips a client filename to a safe basename capped at 255 bytes, without control or
/// bidirectional-override characters, rejecting `.`, `..`, and Windows reserved names to empty:
/// what a file field's [`Uploader`] receives, for a page that parses its own multipart body.
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
    let clean: String = base
        .chars()
        .filter(|&c| !c.is_control() && !is_bidi_control(c))
        .collect();
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

/// Whether `c` reorders the text around it, so `a\u{202e}gnp.exe` would read as `aexe.png`.
fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
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
    fn keep<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<(), String>>;
    fn delete<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<(), String>>;
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

    fn keep<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(Uploader::keep(self, path))
    }

    fn delete<'a>(&'a self, path: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(Uploader::delete(self, path))
    }
}

/// Whether `uploader`, a panel's installed one, still holds `path`; `false` without one.
pub(crate) async fn holds(uploader: Option<&InstalledUploader>, path: &str) -> bool {
    match uploader {
        Some(uploader) => uploader.0.holds(path).await,
        None => false,
    }
}

/// The paths `values` stores under the file field `name`: its one path, or each of its list's,
/// trimmed and none blank.
fn stored_paths<'a>(values: &'a FormValues, name: &str) -> Vec<&'a str> {
    let paths: &[String] = match values.get(name) {
        Some(FormValue::Text(path)) => std::slice::from_ref(path),
        Some(FormValue::List(paths)) => paths,
        _ => &[],
    };
    paths
        .iter()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
        .collect()
}

/// Keeps, through `uploader`, each new path `schema`'s file fields store in `after` and did not in
/// `before`, which a create has none of; only a path the uploader holds.
///
/// # Errors
///
/// The reason the uploader could not keep a path, for the write to fail on.
pub(crate) async fn keep(
    uploader: Option<&InstalledUploader>,
    schema: &Schema,
    before: Option<&FormValues>,
    after: &FormValues,
) -> Result<(), String> {
    let Some(InstalledUploader(uploader)) = uploader else {
        return Ok(());
    };
    for field in schema.fields().filter(|field| field.is_file()) {
        let stored = before.map(|before| stored_paths(before, field.name()));
        for path in stored_paths(after, field.name()) {
            if stored.as_ref().is_some_and(|stored| stored.contains(&path))
                || !uploader.holds(path).await
            {
                continue;
            }
            uploader.keep(path).await?;
        }
    }
    Ok(())
}

/// Deletes, through `uploader`, each file `schema`'s file fields stored in `before` and no longer
/// store in `after`, or every one when `after` is `None`; only a path the uploader holds, and each
/// path once.
pub(crate) async fn discard(
    uploader: Option<&InstalledUploader>,
    schema: &Schema,
    before: &[FormValues],
    after: Option<&FormValues>,
) {
    let Some(InstalledUploader(uploader)) = uploader else {
        return;
    };
    let mut gone = HashSet::new();
    for field in schema.fields().filter(|field| field.is_file()) {
        let kept = after.map(|after| stored_paths(after, field.name()));
        for values in before {
            for path in stored_paths(values, field.name()) {
                if !kept.as_ref().is_some_and(|kept| kept.contains(&path)) {
                    gone.insert(path.to_owned());
                }
            }
        }
    }
    for path in gone {
        if !uploader.holds(&path).await {
            continue;
        }
        if let Err(reason) = uploader.delete(&path).await {
            tracing::error!(path, reason, "an upload the write let go was not deleted");
        }
    }
}

/// Runs `uploader`, a panel's installed one, over this form's file parts, returning inline
/// errors and the fields whose value is now the stored path. A file the field's extensions or
/// size refuse is not stored; it, or a failed store, becomes an inline field error and drops the
/// submitted value. A file list's value becomes the paths it stored, in the order the files
/// posted, whether or not one failed. Call outside the write transaction.
pub(crate) async fn store_uploads(
    uploader: Option<&InstalledUploader>,
    schema: &Schema,
    files: &HashMap<String, Vec<StagedUpload>>,
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
        let label = field.label();
        // A file list holds what it stores, never the names the read gathered.
        if file.multiple {
            values.remove(&name);
        }
        let staged = files.get(&name).map(Vec::as_slice).unwrap_or_default();
        // A file field takes the last file its key posts; a list takes each.
        let staged = match staged.split_last() {
            Some((last, _)) if !file.multiple => std::slice::from_ref(last),
            _ => staged,
        };
        if staged.is_empty() {
            continue;
        }
        let mut paths = Vec::new();
        for staged in staged {
            // A file the field's rules refuse is never stored, nor one whose name sanitizes to
            // nothing, as a Windows device name does.
            let filename = sanitize_filename(&staged.filename);
            let refusal = if filename.is_empty() {
                Some(crate::text!(
                    "tablo-field-upload-name",
                    label = label.clone()
                ))
            } else {
                file.refusal(&label, &filename, staged.bytes.len())
            };
            let stored_path = match refusal {
                Some(refusal) => Err(refusal),
                None => uploader
                    .store(&staged.filename, &staged.bytes)
                    .await
                    .map_err(|reason| {
                        crate::text!(
                            "tablo-field-upload-failed",
                            label = label.clone(),
                            reason = reason
                        )
                    }),
            };
            match stored_path {
                Ok(path) => paths.push(path),
                Err(message) if !errors.contains_key(&name) => errors.add(name.clone(), message),
                Err(_) => {}
            }
        }
        if file.multiple {
            values.insert(name, FormValue::List(paths));
        } else if let Some(path) = paths.pop() {
            values.insert(name.clone(), path);
            stored.insert(name);
        } else {
            values.remove(&name);
        }
    }
    (errors, stored)
}

#[cfg(test)]
mod tests;
