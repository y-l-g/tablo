//! The file list control: a row per stored path, each with its own remove checkbox, the buttons
//! moving it and the handle dragging it, and a file input taking several files at once.

use tablo_ui::{
    ButtonSize, ButtonVariant, button, checkbox as ui_checkbox, input as ui_input,
    label as ui_label,
};
use topcoat::{Result, context::Cx, icon::icon, view::*};

use super::{FileControl, drop_zone, stored_upload_row};
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
        // Moving needs a second row, and `assets/file_lists.js`. The buttons move a row by
        // keyboard, and the handle by pointer.
        let movable = paths.len() > 1 && !disabled;
        let keep_name = format!("keep_{name}");
        let clear_name = format!("clear_{name}");
        let rows: Vec<BoxView<'a>> = paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                let row = stored_upload_row(cx, path.clone(), file.image);
                if disabled {
                    return row;
                }
                let keep_name = keep_name.clone();
                let clear_name = clear_name.clone();
                let clear_id = format!("clear_{id}_{index}");
                // Each row's controls are described by its file, which tells their alike labels
                // apart.
                let file_id = format!("{id}-file-{index}");
                // Square buttons leave the file's name the row's width; the label names each.
                let action = move |id: &'static str,
                                   glyph: topcoat::icon::IconData,
                                   data: Attributes,
                                   described: String| {
                    let label = crate::t!(cx, id);
                    view! {
                        cx =>
                        button(
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::Icon,
                            attrs: attributes! {
                                type="button"
                                aria-label=(label.clone())
                                title=(label)
                                aria-describedby=(described)
                                (data)
                            },
                            icon(
                                data: glyph,
                                attrs: attributes! { class="size-4" aria-hidden="true" }
                            )
                        )
                    }
                };
                let described = file_id.clone();
                let moves = movable.then(move || {
                    view! {
                        cx =>
                        (action(
                            "tablo-files-up",
                            tablo_ui::icons::ARROW_UP,
                            attributes! { cx => data-file-up="" },
                            described.clone(),
                        ))
                        (action(
                            "tablo-files-down",
                            tablo_ui::icons::ARROW_DOWN,
                            attributes! { cx => data-file-down="" },
                            described.clone(),
                        ))
                    }
                });
                let handle = movable.then(|| {
                    view! {
                        cx =>
                        <span
                            class="flex shrink-0 cursor-grab touch-none text-muted-foreground"
                            aria-hidden="true"
                            data-file-handle=""
                        >
                            icon(
                                data: tablo_ui::icons::GRIP_VERTICAL,
                                attrs: attributes! { class="size-4" }
                            )
                        </span>
                    }
                });
                view! {
                    cx =>
                    <div
                        class="flex items-center gap-2 rounded-md bg-background data-[dragging]:opacity-60 data-[dragging]:shadow-md"
                        role="listitem"
                        data-file-item=(path.clone())
                    >
                        if let Some(handle) = handle {
                            (handle)
                        }
                        <div class="min-w-0 flex-1" id=(file_id.clone())>(row)</div>
                        <input type="hidden" name=(keep_name) value=(path.clone())>
                        <div class="flex shrink-0 items-center gap-2">
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
        let list_name = name.clone();
        let input = view! {
            cx =>
            ui_input(
                attrs: attributes! {
                    id=(id)
                    type="file"
                    name=(name)
                    accept=(accept)
                    multiple=(true)
                    disabled=(disabled)
                    aria-invalid=(aria_invalid)
                    aria-describedby=(described_by)
                }
            )
        }
        .boxed();
        let input = if disabled {
            input
        } else {
            drop_zone(cx, file, input, "tablo-files-drop")
        };
        let control = view! {
            cx =>
            <div class="flex flex-col gap-2" role="list" data-file-list=(list_name)>
                for row in rows {
                    (row)
                }
            </div>
            (input)
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
