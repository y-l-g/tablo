# Media uploads: an app-level `Uploader` and a clear control

Date: 2026-09-22 — Status: accepted

## Decision

**The seam is an app-level trait.** `Uploader::store(filename, bytes) -> Result<String, String>`,
installed once with `Panel::uploads(uploader)` and read from app context like `Db`. The trait
returns `impl Future`; a private shim keeps `Panel` non-generic. The framework owns the 10 MiB
body cap, multipart stream, and basename sanitization.

**Bytes buffer; refusals are input.** The body cap bounds memory; the buffer is taken only with an
uploader installed. `Err(reason)` renders `"<Label> could not be uploaded: <reason>"` and
re-renders with submitted values; the reason reaches the user, so it carries no driver message or
path. The framework stores the returned string verbatim and renders it verbatim; the field binds
a `String` and the record-fn contract is unchanged. A file field's value comes only from a file
part, the stored value on an untouched edit, or empty on `clear_<field>`; typed text under the
field name is dropped. A re-render carries the fresh path in hidden `keep_<field>` and reuses it
only when `Uploader::holds(path)` confirms store ownership inside its own root (`false` by
default). A stored value links only when rooted (`/…`, not `//host`) or absolute `http(s)://…`.

**The primitive stops at the file input.** Thumbnails, clear affordances, drag-and-drop, and
progress belong to the media library (ADR-0021), not the generic field.

**The clear control is a transport key.** A file field renders `clear_<field>` with a stored
value; strip-before-record-fn applies, so clearing never reaches the record fn as a write.
Clearing never waives `required`: the value is empty and the record keeps its file unless the
field is `.optional()`.

**`Panel::serve_dir(path, dir)` mounts an app-owned directory** on the app router with hardening
headers keeping files inert on the panel origin. The path is not panel-relative. A served
directory is public: the auth gate covers only the panel prefix and `/_topcoat/runtime`
(ADR-0013). The directory serves `X-Content-Type-Options: nosniff`, a sandboxing
`Content-Security-Policy`, and `Content-Disposition: attachment` except for common raster,
audio/video, or `text/plain` types.

Uploads run before the write transaction and outside it; a rolled-back write never undoes a
store, and an unreferenced file after failed validation wants an app janitor.
