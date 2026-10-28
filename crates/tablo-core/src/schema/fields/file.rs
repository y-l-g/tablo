//! The file control: a file input storing the uploaded file's path, or a list of them, and the
//! extensions, size and count it takes.

use tablo_ui::{checkbox as ui_checkbox, input as ui_input, label as ui_label};
use topcoat::{Result, context::Cx, view::*};

use super::{Field, render_field};
use crate::{FieldRuleFault, Text, text};

/// What a file control takes: the extensions, lowercase and without the dot, the most bytes per
/// file, and for a list the most files. All unset by default, taking any file the form body
/// carries.
#[derive(Clone, Debug, Default)]
pub(crate) struct FileControl {
    pub(crate) extensions: Option<Vec<String>>,
    pub(crate) max_bytes: Option<usize>,
    /// Whether the field holds a list of paths, from [`Field::files`].
    pub(crate) multiple: bool,
    pub(crate) max_files: Option<usize>,
}

impl FileControl {
    /// The sentence refusing a file named `filename`, a sanitized basename, of `size` bytes, as
    /// the field `label`.
    pub(crate) fn refusal(&self, label: &crate::Text, filename: &str, size: usize) -> Option<Text> {
        if !self.takes_name(filename) {
            let listed = self.extensions.as_deref().unwrap_or_default();
            let listed: Vec<String> = listed.iter().map(|ext| format!(".{ext}")).collect();
            return Some(text!(
                "tablo-field-extension",
                label = label,
                count = listed.len(),
                extensions = listed.join(", ")
            ));
        }
        let max = self.max_bytes?;
        (size > max).then(|| {
            text!(
                "tablo-field-max-size",
                label = label,
                max = spelled_size(max)
            )
        })
    }

    /// Whether the control takes a file whose basename is `name`, by its extension.
    pub(crate) fn takes_name(&self, name: &str) -> bool {
        let Some(extensions) = &self.extensions else {
            return true;
        };
        name.rsplit_once('.').is_some_and(|(_, ext)| {
            extensions
                .iter()
                .any(|taken| taken.eq_ignore_ascii_case(ext))
        })
    }

    /// Whether the control takes the file a stored `path` names: its last segment, without a
    /// query or fragment, read as a basename.
    pub(crate) fn takes_path(&self, path: &str) -> bool {
        let path = path.split(['?', '#']).next().unwrap_or_default();
        self.takes_name(path.rsplit('/').next().unwrap_or_default())
    }

    /// The `accept` attribute hinting the extensions to the browser's file picker.
    fn accept(&self) -> Option<String> {
        let extensions = self.extensions.as_ref()?;
        let listed: Vec<String> = extensions.iter().map(|ext| format!(".{ext}")).collect();
        Some(listed.join(","))
    }

    /// What is wrong with the extensions and size the control declares.
    pub(crate) fn rule_faults(&self) -> Vec<FieldRuleFault> {
        let mut faults = Vec::new();
        match self.extensions.as_deref() {
            Some([]) => faults.push(FieldRuleFault::NoExtension),
            Some(extensions) => faults.extend(
                extensions
                    .iter()
                    .filter(|ext| ext.is_empty() || !ext.chars().all(|c| c.is_ascii_alphanumeric()))
                    .map(|ext| FieldRuleFault::InvalidExtension(ext.clone())),
            ),
            None => {}
        }
        if self.max_files.is_some() && !self.multiple {
            faults.push(FieldRuleFault::SingleFile);
        }
        faults
    }

    /// The sentence refusing `count` files as the field `label`, past the most it takes.
    pub(crate) fn count_refusal(&self, label: &crate::Text, count: usize) -> Option<Text> {
        let max = self.max_files?;
        (count > max).then(|| text!("tablo-field-max-files", label = label, count = max))
    }
}

/// `bytes` in the largest binary unit that spells it whole: `2 MiB`, `500 KiB`, `1500 bytes`.
fn spelled_size(bytes: usize) -> Text {
    const KIB: usize = 1024;
    match bytes {
        0 => text!("tablo-size-bytes", count = 0),
        _ if bytes.is_multiple_of(KIB * KIB) => {
            text!("tablo-size-mib", count = bytes / (KIB * KIB))
        }
        _ if bytes.is_multiple_of(KIB) => text!("tablo-size-kib", count = bytes / KIB),
        _ => text!("tablo-size-bytes", count = bytes),
    }
}

impl Field {
    /// The file control's rules, for a file field.
    pub(crate) fn as_file(&self) -> Option<&FileControl> {
        match &self.control {
            super::ControlKind::File(file) => Some(file),
            _ => None,
        }
    }

    /// Whether this is a file field holding a list of paths.
    pub(crate) fn is_file_list(&self) -> bool {
        self.as_file().is_some_and(|file| file.multiple)
    }

    /// Renders a file field's input and its stored path.
    pub(super) fn render_file<'a>(
        &self,
        file: &FileControl,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&crate::values::FieldError>,
        id: String,
        disabled: bool,
    ) -> Result<BoxView<'a>> {
        let name = self.name().to_string();
        // Required only while nothing is stored.
        let stored = stored_path(value);
        let is_edit = stored.is_some();
        let control_required = self.required && !is_edit && !disabled;
        let chrome = self.chrome(cx, id.clone(), error, None);
        let hint_id = format!("{id}-hint");
        // The clear flag is a transport key stripped before any record fn.
        let clear_name = format!("clear_{name}");
        let clear_id = format!("clear_{id}");
        // A value that is not a rooted path or `http(s)` URL renders as text.
        let stored_display: Option<BoxView<'a>> =
            stored.map(|current| stored_upload_row(cx, current));
        let aria_invalid = chrome.aria_invalid();
        let accept = file.accept();
        let described_by = [
            chrome.described_by(),
            (is_edit && !disabled).then(|| hint_id.clone()),
        ]
        .into_iter()
        .flatten()
        .reduce(|ids, id| format!("{ids} {id}"));
        let control = view! {
            cx =>
            if let Some(row) = stored_display {
                (row)
            }
            ui_input(
                attrs: attributes! {
                    id=(id)
                    type="file"
                    name=(name.clone())
                    accept=(accept)
                    required=(control_required)
                    disabled=(disabled)
                    aria-required=(control_required.then_some("true"))
                    aria-invalid=(aria_invalid)
                    aria-describedby=(described_by)
                }
            )
            // A disabled control keeps the stored file, so it offers neither the hint nor clearing.
            if is_edit && !disabled {
                <div class="text-xs text-muted-foreground" id=(hint_id.clone())>
                    (crate::t!(cx, "tablo-file-keep"))
                </div>
                <div class="mt-2 flex items-center gap-2">
                    ui_checkbox(
                        attrs: attributes! {
                            id=(clear_id.clone())
                            name=(clear_name)
                            value="1"
                            disabled=(disabled)
                        }
                    )
                    ui_label(
                        attrs: attributes! { for=(clear_id) class="text-xs text-muted-foreground" },
                        (crate::t!(cx, "tablo-file-remove"))
                    )
                </div>
            }
        }
        .boxed();
        render_field(
            cx,
            &chrome,
            &self.label(),
            control_required,
            attributes! {},
            control,
        )
    }
}

/// The stored path a file field shows, if the value is present and not blank.
fn stored_path(value: Option<&str>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Whether a stored value may become an `href`; a rooted path or absolute `http(s)` URL may,
/// anything else renders as text.
pub(crate) fn is_linkable(path: &str) -> bool {
    is_rooted(path) || {
        let lower = path.trim_start().to_ascii_lowercase();
        is_plain(&lower) && (lower.starts_with("https://") || lower.starts_with("http://"))
    }
}

/// Whether a stored value is a path on this site: rooted, and no protocol-relative URL.
pub(crate) fn is_rooted(path: &str) -> bool {
    let path = path.trim_start();
    is_plain(path) && path.starts_with('/') && !path.starts_with("//")
}

/// Whether `url` holds no backslash or control character. A browser reads `\` as `/` and drops
/// tabs and newlines, so `/\host` or `/<tab>/host` would load from another host.
fn is_plain(url: &str) -> bool {
    !url.chars().any(|c| c == '\\' || c.is_control())
}

/// Signs a stored file's path for the request's router; the first panel mounted on a router
/// installs it, as [`url::file`](crate::url::file).
pub(crate) struct FileHref(pub(crate) fn(&Cx, &str) -> String);

/// The URL a stored file links at: `path` signed when a panel serves it at signed URLs, else
/// `path` itself.
pub(crate) fn file_href(cx: &Cx, path: &str) -> String {
    match topcoat::context::try_context::<FileHref>(cx) {
        Some(FileHref(sign)) => sign(cx, path),
        None => path.to_owned(),
    }
}

/// The `Current: …` row a file field shows for a stored value.
fn stored_upload_row<'a>(cx: &'a Cx, path: String) -> BoxView<'a> {
    let text = path.clone();
    let inner: BoxView<'a> = if is_linkable(&path) {
        let href = file_href(cx, &path);
        view! {
            cx =>
            <a class="font-medium text-foreground underline" href=(href)>(text)</a>
        }
        .boxed()
    } else {
        view! { cx => <span class="font-medium text-foreground">(text)</span> }.boxed()
    };
    view! {
        cx =>
        <div class="text-xs text-muted-foreground" data-file-current=(path)>
            (crate::t!(cx, "tablo-file-current"))
            " "
            (inner)
        </div>
    }
    .boxed()
}

/// A stored upload path, as a link when it is a rooted path or an `http(s)` URL and as text
/// otherwise.
pub(crate) fn stored_upload<'a>(cx: &'a Cx, value: &str) -> BoxView<'a> {
    let path = value.trim().to_string();
    // A value that is not a safe URL is not a link.
    if !is_linkable(&path) {
        return view! {
            cx =>
            <div
                class="text-sm text-foreground font-mono break-all whitespace-pre-wrap"
            >
                (path)
            </div>
        }
        .boxed();
    }
    let href = file_href(cx, &path);
    view! {
        cx =>
        <a
            class="text-sm font-mono break-all whitespace-pre-wrap underline"
            href=(href)
        >
            (path)
        </a>
    }
    .boxed()
}

mod list;

#[cfg(test)]
mod tests;
