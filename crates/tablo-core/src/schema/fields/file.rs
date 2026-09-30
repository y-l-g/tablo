//! The file control: a file input storing the uploaded file's path.

use tablo_ui::{checkbox as ui_checkbox, input as ui_input, label as ui_label};
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::tree::Mode, Field, FieldChrome, ValueKind, render_field, render_value, render_value_view,
};

impl Field {
    /// Render a file field: the stored path as a link in `Mode::View`, the
    /// file input with the stored path and its clear toggle otherwise.
    pub(super) fn render_file<'a>(
        &self,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
        mode: Mode,
    ) -> Result<BoxView<'a>> {
        // The detail page shows the stored path, never a file control: an
        // empty file input on an edit is the panel's "keep the stored file"
        // affordance, which is a statement about a form, not about a record.
        if mode == Mode::View {
            return stored_upload_value(cx, &self.label, value);
        }
        let name = self.name.clone();
        // An edit hydrates the stored path; a create does not. The control
        // is required only when nothing is stored, since a file input cannot
        // be pre-filled.
        let stored = stored_path(value);
        let is_edit = stored.is_some();
        let control_required = self.required && !is_edit;
        let chrome = FieldChrome::new(&name, error, None);
        let hint_id = format!("{name}-hint");
        // The clear flag is a framework transport key, not a field:
        // it names the stored value's owner and is stripped before any record
        // fn, so it can never be written as a field of its own.
        let clear_name = format!("clear_{name}");
        // The stored value as a link to the file it names. Nothing
        // here guesses a URL convention — the app decides what it stores (the
        // uploader's return value) — and a value that is not a rooted path or
        // an `http(s)` URL renders as text rather than as a clickable scheme.
        let stored_display: Option<BoxView<'a>> =
            stored.map(|current| stored_upload_row(cx, current));
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome
            .described_by()
            .or_else(|| is_edit.then(|| hint_id.clone()));
        let control = view! {
            cx =>
            if let Some(row) = stored_display {
                // The stored path is visible, so "there is no file" is no
                // longer ambiguous, and the empty control reads as "leave
                // it alone" rather than "this field is broken".
                (row)
            }
            // The `input` primitive styles `type="file"` through its
            // `file:` classes and carries the `aria-invalid` error styling.
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
                // The one control that says "remove it" rather than "leave
                // it alone". It carries `value="1"` so the
                // framework's own `truthy` vocabulary reads it, and it is a
                // declared transport key, so a generic record fn never sees
                // it.
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

/// The stored path a file field shows, if it has one.
///
/// A value that is absent or only whitespace is not a file: the form renders a
/// create rather than an edit with an empty "Current:" line, and the read-only
/// value renders without a link.
fn stored_path(value: Option<&str>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Whether a stored value may become an `href`: a rooted path
/// (`/uploads/x.png`, not the scheme-relative `//host`) or an absolute
/// `http(s)` URL. Anything else — a bare basename, `javascript:`, `data:` —
/// renders as text: the framework stores what it is handed, so the render is
/// where a scheme is refused.
fn is_linkable(path: &str) -> bool {
    let lower = path.trim_start().to_ascii_lowercase();
    (lower.starts_with('/') && !lower.starts_with("//"))
        || lower.starts_with("https://")
        || lower.starts_with("http://")
}

/// The `Current: …` row a file field shows for a stored value.
///
/// Takes the path by value: the rendered view has to outlive the field's
/// render, and a rendering coroutine may not hold a borrow of it.
fn stored_upload_row<'a>(cx: &'a Cx, path: String) -> BoxView<'a> {
    // The link is the only way to reach the file, and the path is what it
    // says; each node owns its own copy of it.
    let text = path.clone();
    // A value that is not a safe URL renders as plain text: the
    // wrapper and the label stay, only the anchor goes.
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

/// A file field read rather than edited: the label over the stored
/// path, as a link to the file when it is one.
///
/// The reader asks the same "is the stored value right?" question the editor
/// asks, and following the link is how they answer it. `underline` is the
/// affordance: the value sits in body colour inside the field chrome, so
/// without it a stored path reads as text. The chrome is the one
/// `render_value_view` gives every read-only field, so a detail page stays
/// uniform.
fn stored_upload_value<'a>(cx: &'a Cx, label: &str, value: Option<&str>) -> Result<BoxView<'a>> {
    let Some(path) = stored_path(value) else {
        return render_value(cx, label, value, ValueKind::Machine);
    };
    // A value that is not a safe URL is not a link: it renders
    // through the same machine-value path an empty value takes, so the detail
    // page shows the stored text without an `href` to click.
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
