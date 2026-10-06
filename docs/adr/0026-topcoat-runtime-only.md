# Topcoat's runtime is the browser layer

Date: 2026-10-06 — Status: accepted

## Decision

Tables and their dialogs run on Topcoat's runtime: signals, bind attributes, event handlers and
page reruns. Tablo ships no table or dialog script. Where the runtime's expression vocabulary has
no word, a handler escapes to one line of JavaScript (`raw!`): the bulk selection's string
arithmetic, reading the toolbar form's fields, debouncing search, and opening the dialog as a
modal.

- **Tables.** Every table keeps its list query in a signal the page reads on the server, keyed by
  the page path and the table's parameter prefix. A sort or pager link, the toolbar's search and
  filter fields, and a "Clear" link write that signal, and the runtime reruns the page, which runs
  the panel's layers and guards like any request. A rerun waits for the rows instead of showing
  the skeleton. The bulk selection and the confirmation dialog are signals too. There is no
  static table mode and no table shard.
- **Writes.** A delete, a bulk delete and a custom action are plain form posts through the
  table's one write form, which redirect back to the list as it was left. A destructive write
  opens the dialog as a native modal first, which traps focus and closes on Escape or Cancel; its
  submit carries the `confirm=1` the handlers require.
- **No JavaScript.** What works without JavaScript works because it is free: links carry an
  `href` spelling the state they write, and the toolbar is a GET form that Enter submits. The
  dialogs, and so the writes they confirm, need the runtime.
- **Related tables** render on the record's detail page only. A rerun resets every unsaved form
  field but the focused one, so a related table on the edit page would discard the reader's
  edits; tracked upstream as Topcoat form state across reruns.

Tablo still loads three scripts the runtime has no vocabulary for: the theme and sidebar
persistence, and the searchable select, which fetches its options from the server and replaces a
select's markup.

## Rejected

- A table shard per resource and per relation: shards serve at Topcoat's runtime path, so they
  needed a request-body layer to find the panel and string-keyed registries to find the resource,
  and the scope travelled as a packed string. A page rerun reaches the panel's own route.
- Keeping a static table mode beside the live one: every table feature then had two renderers and
  two test matrices.
- Applying a write's response in place with a script: it reimplemented fetch, redirects and
  morphing beside the runtime. A post that redirects back costs one page load and needs no
  client code.
