# Media library: a `medias` table in the showcase

Date: 2026-09-23 — Status: accepted

## Decision

**The library is the app's, and its rows carry no owner.** `tablo-core` keeps the ADR-0017 seam:
a file field binds a `String`, bytes go to the app `Uploader`, the stored value renders as a
link. The library declares `MediaAsset` (`#[table = "medias"]`), one row per stored file with
uploading tenant, store `path`, client `filename`, `kind`, and timestamp.

**One media source.** A post shows one library row as cover through `cover_id`: an optional single
picker storing the picked row's ID. The post form uploads no bytes itself. The public blog resolves
the cover through the row; the admin detail shows reading stats computed from the body.

**The upload form is file-only.** `GET /admin/media` renders the tenant's rows and a file input.
Tenant scoping is the page's own filter on `tenant_id`; a tenantless request is refused.

**The store return is a URL, so it is percent-encoded.** The framework renders the return verbatim
(GH #242); `DirUploader` encodes the name as one RFC 3986 path segment, so every browser-sent name
resolves back to its bytes.

**The rich upload UX is the app's, with a reset fallback.** The page renders file input, preview
region (`data-media-preview`), and a `type="reset"` clear (`data-media-clear`). The script empties
input and preview itself and cancels the reset; it draws the preview (`<img>` from an object URL
for `image/*`, filename otherwise) and revokes the URL on replace or clear. A form with no file
input in reach keeps the browser reset.

The script is the app asset (`MEDIA_JS`), linked `defer`red by the page, not a shell asset:
ADR-0014 owns `tablo-ui` scripts, and the framework provides no app-supplied shell-script seam.
The app owns its script like its `styles.css` (ADR-0006).

**A thumbnail follows content type, not suffix.** `kind` is `"image"` for an `image/*` part,
`"file"` otherwise. Image rows render `<img>`, the rest a link, through one shared view. The
library generates no derivative.

Deleting a media row deletes no bytes; unreferenced-file cleanup stays app-owned (ADR-0017).
