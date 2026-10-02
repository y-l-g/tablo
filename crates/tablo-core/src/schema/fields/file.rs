//! The file control: a file input storing the uploaded file's path.

use tablo_ui::{checkbox as ui_checkbox, input as ui_input, label as ui_label};
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::tree::Mode, Field, FieldChrome, ValueKind, render_field, render_value, render_value_view,
};

impl Field {
    /// Renders a file field's input and its stored path.
    pub(super) fn render_file<'a>(
        &self,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
        mode: Mode,
    ) -> Result<BoxView<'a>> {
        if mode == Mode::View {
            return stored_upload_value(cx, &self.label, value);
        }
        let name = self.name.clone();
        // Required only while nothing is stored.
        let stored = stored_path(value);
        let is_edit = stored.is_some();
        let control_required = self.required && !is_edit;
        let chrome = FieldChrome::new(&name, error, None);
        let hint_id = format!("{name}-hint");
        // The clear flag is a transport key stripped before any record fn.
        let clear_name = format!("clear_{name}");
        // A value that is not a rooted path or `http(s)` URL renders as text.
        let stored_display: Option<BoxView<'a>> =
            stored.map(|current| stored_upload_row(cx, current));
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome
            .described_by()
            .or_else(|| is_edit.then(|| hint_id.clone()));
        let control = view! {
            cx =>
            if let Some(row) = stored_display {
                (row)
            }
            ui_input(
                attrs: attributes! {
                    id=(name.clone())
                    type="file"
                    name=(name.clone())
                    required=(control_required)
                    aria-required=(control_required.then_some("true"))
                    aria-invalid=(aria_invalid)
                    aria-describedby=(described_by)
                }
            )
            if is_edit {
                <div class="text-xs text-muted-foreground" id=(hint_id.clone())>
                    "Leave empty to keep the current file."
                </div>
                <div class="mt-2 flex items-center gap-2">
                    ui_checkbox(
                        attrs: attributes! {
                            id=(clear_name.clone())
                            name=(clear_name.clone())
                            value="1"
                        }
                    )
                    ui_label(
                        attrs: attributes! {
                            for=(clear_name.clone())
                            class="text-xs text-muted-foreground"
                        },
                        "Remove the current file"
                    )
                </div>
            }
        }
        .boxed();
        render_field(
            cx,
            &chrome,
            &self.label,
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

/// Whether a stored value may become an `href`; a rooted path or absolute `http(s)` URL may, anything else renders as text.
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

/// A file field read rather than edited: the label over the stored path, as a link when it is one.
fn stored_upload_value<'a>(cx: &'a Cx, label: &str, value: Option<&str>) -> Result<BoxView<'a>> {
    let Some(path) = stored_path(value) else {
        return render_value(cx, label, value, ValueKind::Machine);
    };
    // A value that is not a safe URL is not a link.
    if !is_linkable(&path) {
        return render_value(cx, label, Some(&path), ValueKind::Machine);
    }
    let href = path.clone();
    let link = view! {
        cx =>
        <a
            class="text-sm font-mono break-all whitespace-pre-wrap underline"
            href=(href)
        >
            (path)
        </a>
    }
    .boxed();
    render_value_view(cx, label, link)
}

#[cfg(test)]
mod tests;
