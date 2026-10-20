# 0026 Topcoat's runtime is the browser layer

Tables and their dialogs run on Topcoat's runtime: each table keeps its list query in a signal,
and a sort, filter, search or page change reruns the page through its own route and guards.
Writes are plain form posts that redirect back to the list; the notification bell's are
procedures ([0028](0028-stored-notifications.md)). Tablo ships no table or dialog
script; a handler escapes to one line of JavaScript only where the runtime has no word, and an
action's input dialog to one more handler, which resets its form as it closes. Links carry an
`href` and the toolbar is a GET form, so reading works without JavaScript; confirmed writes and
an action's input need it. Related tables render on the detail and edit pages: a rerun keeps the
form fields the user edited, but drops repeater rows and fetched options a script added
([#515](https://github.com/y-l-g/tablo/issues/515)).

## Rejected

- A table shard per resource: shards serve at the runtime path, away from the panel's routes.
- A static table mode beside the live one: two renderers and two test matrices.
- Applying a write's response in place: it reimplements fetch, redirects and morphing.
