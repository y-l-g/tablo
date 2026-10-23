# 0017 Uploads go through an app-level `Uploader`

A file field's bytes go to an `Uploader` the app installs once per panel; the framework owns the
body cap, the multipart parsing and filename sanitization, and stores the returned string
verbatim. A panel whose resource form declares a file field and installs no uploader refuses to
mount: storing the filename and dropping the bytes would lose the upload without telling the user.
A refusal is user input, rendered under the field. The field renders a file input and a
clear control, nothing more: previews, drag-and-drop and progress belong to an app's media
library. Uploads run before the write transaction, so a rolled-back write does not undo a store.

`Panel::uploads_dir` installs the stock `DirUploader` over one served directory; any other store
implements `Uploader`. `Panel::serve_dir` serves an app directory with headers that keep files inert on the panel's
origin. A served directory outside the panel prefix is public: the auth gate covers only that
prefix and the runtime (ADR-0013).

The `uploads_dir` directory answers only at a URL the panel signed, with an expiry one to two hours
out: wherever the panel links or shows a stored path it checked the user may see, it signs it,
and an app page signs with `url::file`. A leaked link opens the file until it expires, to anyone.
The signature authorizes a path, not a record: a column over a value users type signs any path they
name, and the random file names are what keep paths unguessable.
Rejected: a route under the prefix per record and field that loads the record through its scoped
query and checks `View` on each request. A stored path renders in columns, relation tables and app
pages that know no owning record, and every `Uploader` would have to stream its bytes back; a
signature needs neither. Rejected too: binding the signature to the browser's CSRF cookie, which a
page streamed before the cookie is set cannot read.
