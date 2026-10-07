//! The media library: the `medias` table and the page that fills it.

use std::collections::HashMap;

use tablo::{
    Ability, NavigationItem, Notification, Page, TenantId, Uploader, csrf, db::db,
    extend::OptionSource, notification::set_notification, require_tenant,
};
use topcoat::{
    Result,
    asset::AssetConfig,
    context::{Cx, try_app_context},
    icon::icon,
    router::{
        content::multipart::Multipart,
        error::{SeeOther, bad_request, see_other},
        route,
    },
    view::{BoxView, View, ViewExt, attributes, view},
};

use crate::{
    app::{DirUploader, basename, upload_dir},
    models::MediaAsset,
};

/// The upload route's path.
pub const MEDIA_PATH: &str = "/admin/media";

/// The widget script the page emits.
pub const MEDIA_JS: topcoat::asset::Asset = topcoat::asset::asset!("../assets/media.js");

/// The `kind` of a row whose bytes are an image.
pub const KIND_IMAGE: &str = "image";

/// The `kind` of every other row.
pub const KIND_FILE: &str = "file";

const FILE_FIELD: &str = "file";

/// One tenant's media rows as a relationship source.
pub struct MediaLibrary;

impl OptionSource for MediaLibrary {
    type Model = MediaAsset;

    fn scoped_query(cx: &Cx) -> Result<toasty::stmt::Query<toasty::stmt::List<MediaAsset>>> {
        let tenant = require_tenant(cx)?;
        Ok(toasty::stmt::Query::<toasty::stmt::List<MediaAsset>>::all()
            .filter(MediaAsset::fields().tenant_id().eq(TenantId::from(tenant))))
    }

    fn allows(_cx: &Cx, ability: Ability<'_, MediaAsset>) -> bool {
        ability.is_read()
    }

    fn requires_tenant(_cx: &Cx) -> bool {
        true
    }

    fn search_expr(_cx: &Cx, term: &str) -> Option<toasty::stmt::Expr<bool>> {
        let term = term.trim();
        if term.is_empty() {
            return None;
        }
        Some(
            MediaAsset::fields()
                .filename()
                .like_with_escape(format!("%{term}%"), '\\'),
        )
    }

    fn order_by(_cx: &Cx) -> Option<toasty::stmt::OrderByExpr> {
        Some(MediaAsset::fields().filename().asc())
    }
}

/// Renders one media row's file.
pub fn media_file_view<'a>(cx: &'a Cx, asset: &MediaAsset) -> BoxView<'a> {
    if asset.kind == KIND_IMAGE {
        let src = asset.path.clone();
        let alt = asset.filename.clone();
        view! {
            cx =>
            <img
                src=(src)
                alt=(alt)
                data-media-thumbnail=""
                class="size-12 shrink-0 rounded-md border border-border object-cover"
            >
        }
        .boxed()
    } else {
        let href = asset.path.clone();
        let name = asset.filename.clone();
        view! {
            cx =>
            <a
                href=(href)
                data-media-link=""
                class="min-w-0 truncate text-sm underline"
            >
                (name)
            </a>
        }
        .boxed()
    }
}

/// Lists every row this tenant holds, and the form that adds one.
pub struct MediaLibraryPage;

impl Page for MediaLibraryPage {
    fn navigation() -> NavigationItem {
        NavigationItem::for_page::<Self>().icon(tablo::ui::icons::IMAGE)
    }

    fn slug() -> String {
        "media".to_string()
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        // Refuses tenantless requests.
        let tenant = require_tenant(cx)?;
        let mut db = db(cx);
        let media = MediaAsset::filter(MediaAsset::fields().tenant_id().eq(TenantId::from(tenant)))
            .order_by(MediaAsset::fields().created_at().desc())
            .exec(&mut db)
            .await?;

        // The form is the app's, so the token is the app's to embed.
        let csrf_token = csrf::ensure_token(cx);
        let has_assets = try_app_context::<AssetConfig>(cx).is_some();

        Ok(view! {
            cx =>
            tablo::ui::page(
                tablo::ui::page_header(
                    tablo::ui::page_title("Media library")
                    tablo::ui::page_description(
                        "Files stored through the app's uploader, picked as post covers."
                    )
                )
                tablo::ui::page_content(
                    tablo::ui::card(
                        tablo::ui::card_header(tablo::ui::card_title("Upload"))
                        tablo::ui::card_content(
                            <form
                                method="post"
                                action=(tablo::url::page::<MediaLibraryPage>(cx))
                                enctype="multipart/form-data"
                                class="flex flex-col gap-4"
                            >
                                (csrf::field(cx, &csrf_token))
                                tablo::ui::field(
                                    tablo::ui::field_label(
                                        attrs: attributes! { for="media-file" },
                                        "File"
                                    )
                                    <div class="flex items-center gap-2">
                                        tablo::ui::input(
                                            attrs: attributes! {
                                                id="media-file"
                                                type="file"
                                                name=(FILE_FIELD)
                                                required=""
                                                data-media-file=""
                                            }
                                        )
                                        tablo::ui::button(
                                            variant: tablo::ui::ButtonVariant::Outline,
                                            size: tablo::ui::ButtonSize::Icon,
                                            attrs: attributes! {
                                                type="reset"
                                                data-media-clear=""
                                                aria-label="Clear the selected file"
                                                title="Clear the selected file"
                                            },
                                            icon(data: tablo::ui::icons::X)
                                        )
                                    </div>
                                    <div
                                        data-media-preview=""
                                        hidden=""
                                        class="flex items-center gap-3 text-xs text-muted-foreground"
                                    ></div>
                                )
                                <div>
                                    tablo::ui::button(
                                        variant: tablo::ui::ButtonVariant::Primary,
                                        attrs: attributes! { type="submit" },
                                        "Upload"
                                    )
                                </div>
                            </form>
                        )
                    )
                    tablo::ui::card(
                        tablo::ui::card_header(tablo::ui::card_title("Stored media"))
                        tablo::ui::card_content(
                            <div class="flex flex-col gap-3">
                                if media.is_empty() {
                                    tablo::ui::empty_state(
                                        title: "No media has been uploaded yet.",
                                        detail: "Uploaded files are listed here.",
                                        attrs: attributes! { data-media-empty="" }
                                    )
                                } else {
                                    <ul
                                        data-media-list=""
                                        class="flex flex-col divide-y divide-border"
                                    >
                                        for asset in &media {
                                            <li
                                                data-media-row=(asset.id.to_string())
                                                class="flex items-center gap-4 py-3"
                                            >
                                                (media_file_view(cx, asset))
                                                <div class="flex min-w-0 flex-col">
                                                    if asset.kind == KIND_IMAGE {
                                                        <span class="truncate text-sm font-medium">
                                                            (asset.filename.clone())
                                                        </span>
                                                    }
                                                    <span class="text-xs text-muted-foreground">
                                                        (asset.created_at.strftime("%Y-%m-%d %H:%M").to_string())
                                                    </span>
                                                </div>
                                            </li>
                                        }
                                    </ul>
                                }
                            </div>
                        )
                    )
                    if has_assets {
                        <script src=(MEDIA_JS) defer=""></script>
                    }
                )
            )
        })
    }
}

/// Stores one uploaded file and writes the row for it.
#[route(POST "/admin/media")]
async fn upload(cx: &Cx, mut multipart: Multipart) -> Result<SeeOther> {
    let tenant = require_tenant(cx)?;
    let mut values = HashMap::new();
    let mut file: Option<UploadedPart> = None;
    while let Some(field) = multipart.next_field().await? {
        let Some(name) = field.name().map(str::to_string) else {
            continue;
        };
        if name == FILE_FIELD {
            let filename = field.file_name().unwrap_or_default().to_string();
            let content_type = field.content_type().unwrap_or_default().to_string();
            let bytes = field.bytes().await?;
            file = Some(UploadedPart {
                filename,
                content_type,
                bytes: bytes.to_vec(),
            });
        } else {
            values.insert(name, field.text().await?);
        }
    }
    csrf::verify(cx, &values)?;
    let part = file.ok_or_else(|| bad_request("Choose a file before uploading."))?;
    let filename = basename(&part.filename);
    if filename.is_empty() || part.bytes.is_empty() {
        return Err(bad_request("Choose a file before uploading.").into());
    }
    let mut db = db(cx);
    let path = DirUploader::new(upload_dir())
        .store(&filename, &part.bytes)
        .await
        .map_err(bad_request)?;
    toasty::create!(MediaAsset {
        tenant_id: TenantId::from(tenant),
        path: path,
        filename: filename,
        kind: kind_of(&part.content_type).to_string(),
        created_at: jiff::Timestamp::now(),
    })
    .exec(&mut db)
    .await?;
    set_notification(cx, Notification::success("Media uploaded"));
    let library =
        tablo::url::page::<MediaLibraryPage>(cx).expect("media page is registered on this panel");
    Ok(see_other(library))
}

/// The file part an upload form submitted, before anything is stored.
struct UploadedPart {
    filename: String,
    content_type: String,
    bytes: Vec<u8>,
}

/// Classifies an uploaded part as image or file.
fn kind_of(content_type: &str) -> &'static str {
    if content_type
        .trim()
        .to_ascii_lowercase()
        .starts_with("image/")
    {
        KIND_IMAGE
    } else {
        KIND_FILE
    }
}

#[cfg(test)]
mod tests;
