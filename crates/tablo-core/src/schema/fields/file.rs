//! The file control: a file input storing the uploaded file's path.

use tablo_ui::{checkbox as ui_checkbox, input as ui_input, label as ui_label};
use topcoat::{Result, context::Cx, view::*};

use super::{Field, render_field};

impl Field {
    /// Renders a file field's input and its stored path.
    pub(super) fn render_file<'a>(
        &self,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
        id: String,
        disabled: bool,
    ) -> Result<BoxView<'a>> {
        let name = self.name().to_string();
        // Required only while nothing is stored.
        let stored = stored_path(value);
        let is_edit = stored.is_some();
        let control_required = self.required && !is_edit && !disabled;
        let chrome = self.chrome(id.clone(), error, None);
        let hint_id = format!("{id}-hint");
        // The clear flag is a transport key stripped before any record fn.
        let clear_name = format!("clear_{name}");
        let clear_id = format!("clear_{id}");
        // A value that is not a rooted path or `http(s)` URL renders as text.
        let stored_display: Option<BoxView<'a>> =
            stored.map(|current| stored_upload_row(cx, current));
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome
            .described_by()
            .or_else(|| (is_edit && !disabled).then(|| hint_id.clone()));
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
                    "Leave empty to keep the current file."
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
                        "Remove the current file"
                    )
                </div>
            }
        }
        .boxed();
        render_field(
            cx,
            &chrome,
            self.label_str(),
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
fn is_linkable(path: &str) -> bool {
    let lower = path.trim_start().to_ascii_lowercase();
    (lower.starts_with('/') && !lower.starts_with("//"))
        || lower.starts_with("https://")
        || lower.starts_with("http://")
}

/// The `Current: …` row a file field shows for a stored value.
fn stored_upload_row<'a>(cx: &'a Cx, path: String) -> BoxView<'a> {
    let text = path.clone();
    let inner: BoxView<'a> = if is_linkable(&path) {
        let href = path.clone();
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
            "Current: "
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
    let href = path.clone();
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

#[cfg(test)]
mod tests;
