//! The file builder's rules: the extensions a file field takes, its most bytes, and a list's most
//! files.

use super::{super::ControlKind, FileField};

impl<F> FileField<F> {
    /// Refuse a file whose name does not end in one of `extensions`, listed without the dot and
    /// matched without case, and hint them to the browser's file picker through `accept`.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Doc { #[key] #[auto] id: uuid::Uuid, cover: String }
    /// # use tablo_core::{Field, lens};
    /// Field::file(lens!(Doc.cover))
    ///     .extensions(["png", "jpg", "jpeg", "webp"])
    ///     .max_bytes(2 * 1024 * 1024);
    /// ```
    ///
    /// - The extension is the check, not the bytes. A served upload's type comes from its
    ///   extension, under `X-Content-Type-Options: nosniff`, so the extension is what decides how a
    ///   browser treats the file: sniffing the content would pass PNG bytes named `page.html`.
    ///   Nothing checks that the bytes are what the extension says; parse them in the
    ///   [`Uploader`](crate::Uploader) when that matters, as before resizing an image.
    /// - The name checked is the sanitized one the [`Uploader`](crate::Uploader) receives. A custom
    ///   uploader that names the stored file otherwise should keep its extension.
    /// - Only the server's check refuses: `accept` is a hint the browser's picker may ignore. A
    ///   refused file is not stored, and the error renders on the field.
    /// - A refused submission carries a stored upload back through `keep_<field>`, signed for the
    ///   field, so a carry names only a file this field's rules passed; a carried path whose
    ///   extension the field does not take is dropped too, as one the uploader does not hold is. A
    ///   value the record already stores is not checked again.
    /// - Mounting refuses an empty list, and an extension that is not letters and digits.
    pub fn extensions<I>(mut self, extensions: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let extensions = extensions
            .into_iter()
            .map(|ext| {
                let ext = ext.as_ref().trim();
                ext.strip_prefix('.').unwrap_or(ext).to_ascii_lowercase()
            })
            .collect();
        if let ControlKind::File(file) = &mut self.0.control {
            file.extensions = Some(extensions);
        }
        self
    }

    /// Refuse a file larger than `max_bytes`; the error spells the size in KiB or MiB when it
    /// divides evenly. Only the server checks it, before the [`Uploader`](crate::Uploader) stores
    /// the file; a carried upload passed it when it was stored. Mounting refuses a size at or
    /// above the panel's [`Limits::body_bytes`](crate::Limits::body_bytes), which refuses a larger
    /// body first.
    pub fn max_bytes(mut self, max_bytes: usize) -> Self {
        if let ControlKind::File(file) = &mut self.0.control {
            file.max_bytes = Some(max_bytes);
        }
        self
    }

    /// Refuse a submission leaving a [`Field::files`](crate::Field::files) list with more than
    /// `count` files: the stored ones it keeps and the ones it adds. The error renders on the
    /// field, and the files the submission stored are carried to the next one, so the user
    /// removes some and saves again. Mounting refuses it on a field over one path.
    pub fn max_files(mut self, count: usize) -> Self {
        if let ControlKind::File(file) = &mut self.0.control {
            file.max_files = Some(count);
        }
        self
    }
}
