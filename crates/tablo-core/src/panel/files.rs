//! The panel's file storage: the uploader its file fields store through, and the directories it
//! serves.

use std::path::PathBuf;

use super::{Panel, build::is_directory_pattern, headers};
use crate::{DeclarationError, DeclarationErrorKind};

impl Panel {
    /// Installs the [`Uploader`](crate::Uploader) this panel's file fields store through; without
    /// one a panel whose resource form declares a file field refuses to mount.
    pub fn uploads(mut self, uploader: impl crate::Uploader) -> Self {
        self.uploads = Some(crate::upload::InstalledUploader::new(uploader));
        self
    }

    /// Stores file fields' uploads in `dir` and serves them at `url_prefix` (`"/uploads"`), through
    /// a [`DirUploader`](crate::upload::DirUploader).
    ///
    /// ```no_run
    /// # use tablo_core::Panel;
    /// Panel::new("admin").uploads_dir("/uploads", "var/uploads");
    /// ```
    ///
    /// The directory answers only at a signed URL that expires, so a file's bare URL opens
    /// nothing and a leaked link stops working within two hours. The panel signs a file field's
    /// link, a [`FileColumn`](crate::FileColumn)'s and an [`ImageColumn`](crate::ImageColumn)'s
    /// thumbnail when it renders them, on a page whose tenant and policy let the user see the
    /// record; a page of the app's own signs a stored path with [`url::file`](crate::url::file).
    /// The key derives from the router's cookie `Key`, else from one drawn per process, so
    /// without a shared `Key` a link opens only on the instance that rendered it. Any other
    /// request answers 404. The files carry [`serve_dir`](Self::serve_dir)'s hardening headers.
    ///
    /// A file a record form stored for a submission that never saved is deleted after 24 hours:
    /// [`DirUploader::expire_pending`](crate::upload::DirUploader::expire_pending).
    pub fn uploads_dir(self, url_prefix: &str, dir: impl Into<PathBuf>) -> Self {
        self.uploads_dir_with(url_prefix, dir, |uploader| uploader)
    }

    /// [`uploads_dir`](Self::uploads_dir), with the [`DirUploader`](crate::upload::DirUploader)
    /// wrapped in an app's own [`Uploader`](crate::Uploader), such as one that checks the bytes
    /// before it stores them: the files still answer only at signed URLs, and pending ones still
    /// expire. The wrapper forwards [`keep`](crate::Uploader::keep), and
    /// [`delete`](crate::Uploader::delete) to delete what a write lets go.
    pub fn uploads_dir_with<U: crate::Uploader>(
        self,
        url_prefix: &str,
        dir: impl Into<PathBuf>,
        wrap: impl FnOnce(crate::upload::DirUploader) -> U,
    ) -> Self {
        let url_prefix = url_prefix.trim_end_matches('/');
        let dir = dir.into();
        self.serve(format!("{url_prefix}/{{*file}}"), dir.clone(), true)
            .uploads(wrap(
                crate::upload::DirUploader::new(url_prefix, dir)
                    .expire_pending(std::time::Duration::from_secs(24 * 3600)),
            ))
    }

    /// Serves the directory `dir` at route pattern `path`, which must end in a catch-all.
    ///
    /// The files share the panel's origin, so every file response carries hardening headers:
    ///
    /// - `X-Content-Type-Options: nosniff`;
    /// - a sandboxing `Content-Security-Policy`, which is fixed;
    /// - `Content-Disposition: attachment`, unless the file is a common image, audio or video type
    ///   or plain text, so an uploaded HTML or SVG file downloads instead of running script.
    ///
    /// Serve active documents from another origin. The directory is **public**: the auth gate does
    /// not cover it, unless `path` sits under the panel prefix, which the gate covers. Serve
    /// uploads with [`uploads_dir`](Self::uploads_dir) instead, whose URLs expire.
    pub fn serve_dir(self, path: impl Into<String>, dir: impl Into<PathBuf>) -> Self {
        self.serve(path.into(), dir.into(), false)
    }

    /// Serves `dir` at `path`, at signed URLs only when `signed`.
    fn serve(mut self, path: String, dir: PathBuf, signed: bool) -> Self {
        if !is_directory_pattern(&path) {
            self.configuration_errors.push(DeclarationError::panel(
                DeclarationErrorKind::ServeDir(crate::ServeDirFault::WithoutCatchAll {
                    path: path.clone(),
                }),
            ));
        }
        self.served_dirs
            .push(headers::ServedDir { path, dir, signed });
        self
    }
}

/// The patterns of `served`, and the roots of those that need a signed URL.
pub(super) fn served_paths(served: &[headers::ServedDir]) -> (Vec<String>, Vec<String>) {
    let paths = served.iter().map(|served| served.path.clone()).collect();
    let signed = served
        .iter()
        .filter(|served| served.signed)
        .map(|served| served_root(&served.path).to_owned())
        .collect();
    (paths, signed)
}

/// A served directory's pattern without its catch-all's name: the router treats
/// `/uploads/{*file}` and `/uploads/{*path}` as one route.
pub(super) fn served_root(path: &str) -> &str {
    path.rsplit_once("{*").map_or(path, |(root, _)| root)
}
