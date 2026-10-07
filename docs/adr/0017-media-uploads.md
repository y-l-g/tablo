# 0017 Uploads go through an app-level `Uploader`

A file field's bytes go to an `Uploader` the app installs once per panel; the framework owns the
body cap, the multipart parsing and filename sanitization, and stores the returned string
verbatim. A refusal is user input, rendered under the field. The field renders a file input and a
clear control, nothing more: previews, drag-and-drop and progress belong to an app's media
library. Uploads run before the write transaction, so a rolled-back write does not undo a store.

`Panel::serve_dir` serves an app directory with headers that keep files inert on the panel's
origin. A served directory is public: the auth gate covers only the panel prefix and the runtime
(ADR-0013).
