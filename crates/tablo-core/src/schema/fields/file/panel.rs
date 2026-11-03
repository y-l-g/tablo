//! The panel a file field renders: the drop area over its input, and its files as image tiles or
//! rows, each with the controls removing and moving it.

use topcoat::{context::Cx, icon::icon, view::*};

use super::{file_href, is_linkable};

/// The last segment of a stored path, without its query or fragment: the name a file shows.
fn basename(path: &str) -> String {
    let path = path.split(['?', '#']).next().unwrap_or_default();
    match path.trim_end_matches('/').rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => path.to_owned(),
    }
}

/// The classes of a round button over a file: light on an image's dark header, muted on a row.
fn round_button(image: bool) -> &'static str {
    if image {
        "flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-full bg-black/50 text-white transition-colors hover:bg-black/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-white"
    } else {
        "flex size-6 shrink-0 cursor-pointer items-center justify-center rounded-full bg-foreground/10 text-foreground transition-colors hover:bg-foreground/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring"
    }
}

/// The classes of the files a panel lists, stored and picked in one flow: a grid of tiles for
/// images, a column of rows else. It hides while it lists none.
fn files_class(image: bool) -> &'static str {
    if image {
        "grid grid-cols-2 gap-2 not-has-[[data-file-tile]]:hidden @md:grid-cols-3 @2xl:grid-cols-4"
    } else {
        "flex flex-col gap-1.5 not-has-[[data-file-tile]]:hidden"
    }
}

/// The classes of one file a panel lists, stored or picked. A file its remove toggle drops
/// fades.
fn tile_class(image: bool) -> &'static str {
    if image {
        "group/tile relative aspect-[4/3] overflow-hidden rounded-md bg-muted ring-1 ring-foreground/10 transition-opacity has-[[data-file-clear]:checked]:opacity-50 data-[dragging]:z-10 data-[dragging]:shadow-lg data-[dragging]:ring-2 data-[dragging]:ring-primary"
    } else {
        "group/tile flex min-h-10 items-center gap-2.5 rounded-md bg-muted px-2 py-1.5 text-sm transition-opacity has-[[data-file-clear]:checked]:opacity-50 data-[dragging]:z-10 data-[dragging]:shadow-lg data-[dragging]:ring-2 data-[dragging]:ring-primary"
    }
}

/// The icon standing for a file with no thumbnail.
fn file_icon<'a>(cx: &'a Cx, class: &'static str) -> BoxView<'a> {
    view! {
        cx =>
        icon(
            data: tablo_ui::icons::FILE_TEXT,
            attrs: attributes! { class=(class) aria-hidden="true" }
        )
    }
    .boxed()
}

/// A stored file a panel lists: an image's thumbnail under a header, or a row, holding its name
/// linked to it and the controls removing and moving it.
pub(super) struct StoredTile<'a> {
    pub(super) path: String,
    /// The id of the file's name, which describes its controls.
    pub(super) name_id: String,
    pub(super) image: bool,
    /// The toggle dropping the file, from [`remove_toggle`].
    pub(super) remove: Option<BoxView<'a>>,
    /// The buttons moving a list's file by keyboard, from [`move_button`].
    pub(super) moves: Option<BoxView<'a>>,
    /// The `keep_<field>` key a list's file posts its path under, which keeps it in place.
    pub(super) keep: Option<String>,
    /// Whether a pointer drags the file; `assets/file_lists.js` does it.
    pub(super) movable: bool,
}

impl<'a> StoredTile<'a> {
    pub(super) fn render(self, cx: &'a Cx) -> BoxView<'a> {
        let Self {
            path,
            name_id,
            image,
            remove,
            moves,
            keep,
            movable,
        } = self;
        // A value that is not a rooted path or `http(s)` URL renders whole, as text.
        let href = is_linkable(&path).then(|| file_href(cx, &path));
        let thumbnail = href.clone().filter(|_| image);
        let shown = basename(&path);
        let title = path.clone();
        let name: BoxView<'a> = match href {
            Some(href) => view! {
                cx =>
                <a
                    class="block truncate font-medium hover:underline"
                    href=(href)
                    title=(title)
                >
                    (shown)
                </a>
            }
            .boxed(),
            None => {
                view! { cx => <span class="block font-medium break-all">(title)</span> }.boxed()
            }
        };
        let grip = movable.then(|| {
            view! {
                cx =>
                <span
                    class="flex shrink-0 cursor-grab touch-none opacity-70"
                    aria-hidden="true"
                    data-file-grip=""
                >
                    icon(
                        data: tablo_ui::icons::GRIP_VERTICAL,
                        attrs: attributes! { class="size-4" }
                    )
                </span>
            }
        });
        let body: BoxView<'a> = if image {
            // The name in the header says what the image is.
            let media: BoxView<'a> = match thumbnail {
                Some(src) => view! {
                    cx =>
                    <img
                        class="absolute inset-0 size-full object-cover group-has-[[data-file-clear]:checked]/tile:grayscale"
                        src=(src)
                        alt=""
                        loading="lazy"
                        draggable="false"
                    >
                }
                .boxed(),
                None => view! {
                    cx =>
                    <div
                        class="absolute inset-0 flex items-center justify-center text-muted-foreground"
                    >
                        (file_icon(cx, "size-8"))
                    </div>
                }
                .boxed(),
            };
            view! {
                cx =>
                (media)
                <div
                    class="absolute inset-x-0 top-0 flex items-start gap-2 bg-linear-to-b from-black/70 to-transparent p-2 pb-8 text-xs text-white"
                >
                    if let Some(remove) = remove {
                        (remove)
                    }
                    <div
                        class="min-w-0 flex-1 pt-1 group-has-[[data-file-clear]:checked]/tile:line-through"
                        id=(name_id)
                    >
                        (name)
                    </div>
                    if let Some(grip) = grip {
                        (grip)
                    }
                </div>
                if let Some(moves) = moves {
                    <div
                        class="absolute end-2 bottom-2 flex gap-1 opacity-0 transition-opacity group-focus-within/tile:opacity-100 group-hover/tile:opacity-100"
                    >
                        (moves)
                    </div>
                }
            }
            .boxed()
        } else {
            view! {
                cx =>
                if let Some(remove) = remove {
                    (remove)
                }
                (file_icon(cx, "size-4 shrink-0 text-muted-foreground"))
                <div
                    class="min-w-0 flex-1 text-foreground group-has-[[data-file-clear]:checked]/tile:line-through"
                    id=(name_id)
                >
                    (name)
                </div>
                if let Some(moves) = moves {
                    <div class="flex shrink-0 gap-1">(moves)</div>
                }
                if let Some(grip) = grip {
                    <span class="text-muted-foreground">(grip)</span>
                }
            }
            .boxed()
        };
        let class = if movable {
            format!("{} cursor-grab active:cursor-grabbing", tile_class(image))
        } else {
            tile_class(image).to_owned()
        };
        let item = keep.as_ref().map(|_| path.clone());
        let keep = keep.map(|keep| (keep, path.clone()));
        view! {
            cx =>
            <div
                class=(class)
                role=(item.is_some().then_some("listitem"))
                data-file-tile=""
                data-file-item=(item)
                data-file-handle=(movable)
                data-file-current=(path)
            >
                (body)
                if let Some((keep, value)) = keep {
                    <input type="hidden" name=(keep) value=(value)>
                }
            </div>
        }
        .boxed()
    }
}

/// The round toggle over a stored file that posts `value` under `name` to drop it, its X turning
/// into an undo while checked. `label`, a message id, names it.
pub(super) fn remove_toggle<'a>(
    cx: &'a Cx,
    name: String,
    value: String,
    id: String,
    described: String,
    image: bool,
    label: &'static str,
) -> BoxView<'a> {
    let label = crate::t!(cx, label);
    view! {
        cx =>
        <label class=(round_button(image)) title=(label.clone())>
            <input
                class="peer sr-only"
                type="checkbox"
                id=(id)
                name=(name)
                value=(value)
                aria-describedby=(described)
                data-file-clear=""
            >
            icon(
                data: tablo_ui::icons::X,
                attrs: attributes! { class="size-3.5 peer-checked:hidden" aria-hidden="true" }
            )
            icon(
                data: tablo_ui::icons::UNDO_2,
                attrs: attributes! {
                    class="hidden size-3.5 peer-checked:block"
                    aria-hidden="true"
                }
            )
            <span class="sr-only">(label)</span>
        </label>
    }
    .boxed()
}

/// A round button moving a list's file one place earlier (`up`) or later; its file, `described`,
/// tells it from its neighbours' alike buttons.
pub(super) fn move_button<'a>(cx: &'a Cx, up: bool, described: String, image: bool) -> BoxView<'a> {
    let id = if up {
        "tablo-files-up"
    } else {
        "tablo-files-down"
    };
    let label = crate::t!(cx, id);
    // A grid reads across, a column down.
    let glyph = match (image, up) {
        (true, true) => tablo_ui::icons::ARROW_LEFT,
        (true, false) => tablo_ui::icons::ARROW_RIGHT,
        (false, true) => tablo_ui::icons::ARROW_UP,
        (false, false) => tablo_ui::icons::ARROW_DOWN,
    };
    view! {
        cx =>
        <button
            class=(round_button(image))
            type="button"
            aria-label=(label.clone())
            title=(label)
            aria-describedby=(described)
            data-file-up=(up)
            data-file-down=(!up)
        >
            icon(
                data: glyph,
                attrs: attributes! { class="size-3.5" aria-hidden="true" }
            )
        </button>
    }
    .boxed()
}

/// A file picked and not saved yet, as `assets/file_lists.js` copies it per file: the stored
/// file's look, with its size and a button taking it back out.
fn picked_tile<'a>(cx: &'a Cx, image: bool) -> BoxView<'a> {
    let label = crate::t!(cx, "tablo-files-remove");
    let unpick = view! {
        cx =>
        <button
            class=(round_button(image))
            type="button"
            aria-label=(label.clone())
            title=(label)
            data-file-unpick=""
        >
            icon(
                data: tablo_ui::icons::X,
                attrs: attributes! { class="size-3.5" aria-hidden="true" }
            )
        </button>
    };
    if image {
        view! {
            cx =>
            <div class=(tile_class(true)) role="listitem" data-file-tile="">
                <div
                    class="absolute inset-0 flex items-center justify-center text-muted-foreground"
                >
                    (file_icon(cx, "size-8"))
                </div>
                <img
                    class="absolute inset-0 size-full object-cover"
                    alt=""
                    hidden=(true)
                    data-file-picked-thumb=""
                >
                <div
                    class="absolute inset-x-0 top-0 flex items-start gap-2 bg-linear-to-b from-black/70 to-transparent p-2 pb-8 text-xs text-white"
                >
                    (unpick)
                    <div class="min-w-0 flex-1">
                        <div class="truncate font-medium" data-file-picked-name=""></div>
                        <div class="opacity-80" data-file-picked-size=""></div>
                    </div>
                </div>
            </div>
        }
        .boxed()
    } else {
        view! {
            cx =>
            <div class=(tile_class(false)) role="listitem" data-file-tile="">
                (unpick)
                (file_icon(cx, "size-4 shrink-0 text-muted-foreground"))
                <div
                    class="min-w-0 flex-1 truncate font-medium"
                    data-file-picked-name=""
                ></div>
                <div
                    class="shrink-0 text-xs text-muted-foreground"
                    data-file-picked-size=""
                ></div>
            </div>
        }
        .boxed()
    }
}

/// The classes of a panel's file input: invisible over the drop area, so a click on the area
/// opens the browser's picker and a drop on it fills the input.
pub(super) const INPUT_CLASS: &str = "absolute inset-0 size-full cursor-pointer opacity-0 file:cursor-pointer disabled:cursor-not-allowed";

/// The panel a file field renders: the drop area over its `input`, saying `drop` (a message id),
/// then the `stored` files and, after them, the ones `assets/file_lists.js` lists as picked and not
/// saved yet, and the field's `hint`. The browser sends the input's files, so without the script a
/// click on the area still picks them.
pub(super) fn upload_panel<'a>(
    cx: &'a Cx,
    image: bool,
    input: BoxView<'a>,
    drop: &'static str,
    stored: Option<BoxView<'a>>,
    hint: Option<BoxView<'a>>,
) -> BoxView<'a> {
    view! {
        cx =>
        <div
            class="@container flex flex-col gap-2 rounded-lg border border-border bg-background p-2 shadow-xs transition-colors has-[:disabled]:bg-muted/40 has-[[aria-invalid=true]]:border-destructive data-[dragging]:border-primary data-[dragging]:bg-primary/5"
            data-file-drop=""
            data-file-image=(image)
        >
            <div
                class="relative flex flex-col items-center gap-1 rounded-md px-3 py-4 text-center text-sm text-muted-foreground transition-colors hover:bg-muted/60 has-[:focus-visible]:ring-2 has-[:focus-visible]:ring-ring has-[:disabled]:opacity-60 has-[:disabled]:hover:bg-transparent"
            >
                icon(
                    data: tablo_ui::icons::UPLOAD,
                    attrs: attributes! { class="size-5" aria-hidden="true" }
                )
                // The input's label is the field's; this only shows what a click or drop does.
                <span aria-hidden="true">
                    (crate::t!(cx, drop))
                    " "
                    <span class="font-medium text-primary">
                        (crate::t!(cx, "tablo-file-browse"))
                    </span>
                </span>
                (input)
            </div>
            // Each list is `contents`, so its files flow in the panel's grid.
            <div class=(files_class(image))>
                if let Some(stored) = stored {
                    (stored)
                }
                <div class="contents" role="list" data-file-pending=""></div>
            </div>
            if let Some(hint) = hint {
                (hint)
            }
            <template data-file-picked-template="">(picked_tile(cx, image))</template>
        </div>
    }
    .boxed()
}
