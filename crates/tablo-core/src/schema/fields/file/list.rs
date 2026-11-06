//! The file list control: a tile per stored path, each with its own remove toggle, the buttons
//! moving it and the drag moving it, and a file input taking several files at once.

use topcoat::{Result, context::Cx, view::*};

use super::{
    FileControl,
    panel::{INPUT_CLASS, StoredTile, move_button, remove_toggle, upload_panel},
};
use crate::schema::fields::{Field, render_field};

impl Field {
    /// Renders a file list field's stored paths and its input.
    ///
    /// Each tile posts its path back under `keep_<field>`, so the submission keeps the tiles in
    /// the order it posts them; its toggle posts the path under `clear_<field>` to drop it, which
    /// is the only way a submission drops a stored file.
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
        let image = file.image;
        let paths: Vec<String> = paths
            .iter()
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
            .collect();
        let editable = !paths.is_empty() && !disabled;
        // Moving needs a second file, and `assets/file_lists.js`. The buttons move a file by
        // keyboard, and a drag by pointer.
        let movable = paths.len() > 1 && !disabled;
        let keep_name = format!("keep_{name}");
        let clear_name = format!("clear_{name}");
        let tiles: Vec<BoxView<'a>> = paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| {
                // Each file's controls are described by its name, which tells their alike labels
                // apart.
                let name_id = format!("{id}-file-{index}");
                let remove = (!disabled).then(|| {
                    remove_toggle(
                        cx,
                        clear_name.clone(),
                        path.clone(),
                        format!("clear_{id}_{index}"),
                        name_id.clone(),
                        image,
                        "tablo-files-remove",
                    )
                });
                let moves = movable.then(|| {
                    let (up, down) = (
                        move_button(cx, true, name_id.clone(), image),
                        move_button(cx, false, name_id.clone(), image),
                    );
                    view! {
                        (up)
                        (down)
                    }
                    .boxed()
                });
                StoredTile {
                    path,
                    name_id,
                    image,
                    remove,
                    moves,
                    keep: (!disabled).then(|| keep_name.clone()),
                    movable,
                }
                .render(cx)
            })
            .collect();
        let described_by = [chrome.described_by(), editable.then(|| hint_id.clone())]
            .into_iter()
            .flatten()
            .reduce(|ids, id| format!("{ids} {id}"));
        let aria_invalid = chrome.aria_invalid();
        let accept = file.accept();
        let list_name = name.clone();
        let stored = view! {
            <div class="contents" role="list" data-file-list=(list_name)>
                for tile in tiles {
                    (tile)
                }
            </div>
        }
        .boxed();
        let input = view! {
            <input
                class=(INPUT_CLASS)
                id=(id)
                type="file"
                name=(name)
                accept=(accept)
                multiple=(true)
                disabled=(disabled)
                aria-invalid=(aria_invalid)
                aria-describedby=(described_by)
            >
        }
        .boxed();
        let hint = editable.then(|| {
            view! {
                <div class="sr-only" id=(hint_id)>
                    (crate::t!(cx, "tablo-files-add"))
                </div>
            }
            .boxed()
        });
        let control = upload_panel(cx, image, input, "tablo-files-drop", Some(stored), hint);
        render_field(cx, &chrome, &self.label(), false, attributes! {}, control)
    }
}
