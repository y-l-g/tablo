//! The file list control: a row per stored path, each with its own remove checkbox and the
//! buttons moving it, and a file input taking several files at once.

use tablo_ui::{
    ButtonSize, ButtonVariant, button, checkbox as ui_checkbox, input as ui_input,
    label as ui_label,
};
use topcoat::{Result, context::Cx, view::*};

use super::{FileControl, stored_upload_row};
use crate::schema::fields::{Field, render_field};

impl Field {
    /// Renders a file list field's stored paths and its input.
    ///
    /// Each row posts its path back under `keep_<field>`, so the submission keeps the rows in the
    /// order it posts them; its checkbox posts the path under `clear_<field>` to drop it, which is
    /// the only way a submission drops a stored file.
    pub(in crate::schema::fields) fn render_files<'a>(
        &self,
        file: &FileControl,
        cx: &'a Cx,
        paths: &[String],
        error: Option<&crate::values::FieldError>,
        id: String,
        disabled: bool,
    ) -> Result<BoxView<'a>> {
        let name = self.name().to_string();
        let chrome = self.chrome(cx, id.clone(), error, None);
        let hint_id = format!("{id}-hint");
        let paths: Vec<String> = paths
            .iter()
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
            .collect();
        let editable = !paths.is_empty() && !disabled;
        // Moving needs a second row, and `assets/file_lists.js`.
        let movable = paths.len() > 1 && !disabled;
        let keep_name = format!("keep_{name}");
        let clear_name = format!("clear_{name}");
        let rows: Vec<BoxView<'a>> = paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                let row = stored_upload_row(cx, path.clone());
                if disabled {
                    return row;
                }
                let keep_name = keep_name.clone();
                let clear_name = clear_name.clone();
                let clear_id = format!("clear_{id}_{index}");
                // Each row's controls are described by its file, which tells their alike labels
                // apart.
                let file_id = format!("{id}-file-{index}");
                let action = move |id: &'static str, data: Attributes, described: String| {
                    view! {
                        cx =>
                        button(
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Sm,
                            attrs: attributes! { type="button" aria-describedby=(described) (data) },
                            (crate::t!(cx, id))
                        )
                    }
                };
                let described = file_id.clone();
                let moves = movable.then(move || {
                    view! {
                        cx =>
                        (action(
                            "tablo-files-up",
                            attributes! { cx => data-file-up="" },
                            described.clone(),
                        ))
                        (action(
                            "tablo-files-down",
                            attributes! { cx => data-file-down="" },
                            described.clone(),
                        ))
                    }
                });
                view! {
                    cx =>
                    <div
                        class="flex items-center gap-3"
                        role="listitem"
                        data-file-item=(path.clone())
                    >
                        <span id=(file_id.clone())>(row)</span>
                        <input type="hidden" name=(keep_name) value=(path.clone())>
                        <div class="flex items-center gap-2">
                            ui_checkbox(
                                attrs: attributes! {
                                    id=(clear_id.clone())
                                    name=(clear_name)
                                    value=(path)
                                    aria-describedby=(file_id)
                                }
                            )
                            ui_label(
                                attrs: attributes! { for=(clear_id) class="text-xs text-muted-foreground" },
                                (crate::t!(cx, "tablo-files-remove"))
                            )
                        </div>
                        if let Some(moves) = moves {
                            (moves)
                        }
                    </div>
                }
                .boxed()
            })
            .collect();
        let described_by = [chrome.described_by(), editable.then(|| hint_id.clone())]
            .into_iter()
            .flatten()
            .reduce(|ids, id| format!("{ids} {id}"));
        let aria_invalid = chrome.aria_invalid();
        let accept = file.accept();
        let control = view! {
            cx =>
            <div class="flex flex-col gap-1" role="list" data-file-list=(name.clone())>
                for row in rows {
                    (row)
                }
            </div>
            ui_input(
                attrs: attributes! {
                    id=(id)
                    type="file"
                    name=(name.clone())
                    accept=(accept)
                    multiple=(true)
                    disabled=(disabled)
                    aria-invalid=(aria_invalid)
                    aria-describedby=(described_by)
                }
            )
            if editable {
                <div class="text-xs text-muted-foreground" id=(hint_id)>
                    (crate::t!(cx, "tablo-files-add"))
                </div>
            }
        }
        .boxed();
        render_field(cx, &chrome, &self.label(), false, attributes! {}, control)
    }
}
