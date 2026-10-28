# 0017 Uploads go through an app-level `Uploader`

A file field's bytes go to an `Uploader` the app installs once per panel; the framework owns the
body cap, the multipart parsing and filename sanitization, and stores the returned string
verbatim. A panel whose resource form declares a file field and installs no uploader refuses to
mount: storing the filename and dropping the bytes would lose the upload without telling the user.
A refusal is user input, rendered under the field. The field renders a file input and a
clear control, nothing more: previews, drag-and-drop and progress belong to an app's media
library. Uploads run before the write transaction, so a rolled-back write does not undo a store.

A file list binds a `Vec<String>` column: one path per file, in the order the form lists them, its
input taking several files at once. Each stored file renders as a row posting its bare path back,
which only orders the list, with its own remove checkbox, so the list adds and removes files
without JavaScript. A save starts from the list the record stores inside its transaction: only a
remove checkbox drops a path, a posted path the record does not store is ignored, a stored path no
row posts stays after the posted ones, and the posted rows keep the order they post in. So a stale
form or a row a script left out never deletes a file another save added. Moving a row is
`file_lists.js` moving its element, so the order needs no key of its own and the browser renumbers
nothing; without JavaScript the rows keep their stored order. A refused submission's uploads are
carried under the tag a single file field's carry uses, beside the rows. A list choosing more files
than `max_files` stores none of them.

`Panel::uploads_dir` installs the stock `DirUploader` over one served directory; any other store
implements `Uploader`. `Panel::serve_dir` serves an app directory with headers that keep files inert on the panel's
origin. A served directory outside the panel prefix is public: the auth gate covers only that
prefix and the runtime (ADR-0013).

The `uploads_dir` directory answers only at a URL the panel signed, with an expiry one to two hours
out: wherever the panel links or shows a stored path it checked the user may see, it signs it,
and an app page signs with `url::file`. A leaked link opens the file until it expires, to anyone.
The signature authorizes a path, not a record: a column over a value users type signs any path they
name, and the random file names are what keep paths unguessable.
Rejected for the file list: storing the paths joined in one `String`, which every reader splits
again; a related table of files per record, which each app's media library already is; a file
field inside a repeater's item, which reads a submission's files per row for what a list of paths
holds; a save keeping exactly the rows it posts, which deletes every stored file when a row's
tag fails to open after a restart and drops a file another save added; a tag on each stored row,
which binds no record, so a browser that once saw a row could keep its path on any record it
edits; and an order key beside the rows, as a repeater's, which says again what the rows' posting
order already does.

Rejected: a route under the prefix per record and field that loads the record through its scoped
query and checks `View` on each request. A stored path renders in columns, relation tables and app
pages that know no owning record, and every `Uploader` would have to stream its bytes back; a
signature needs neither. Rejected too: binding the signature to the browser's CSRF cookie, which a
page streamed before the cookie is set cannot read.
