# 0038 A creatable choice frames the related resource's create page

A relationship choice declared `.creatable()` renders a link to the related resource's create
page beside its select, and a dialog holding an empty frame. A script loads the create page into
the frame with `?pick=1`, which renders it without the panel's shell and makes a save land on
`{list}/-/picked?key=` instead of the list. That page reads the record back through the resource's
scoped query under its `ViewAny`, as its options are, and carries its key and title. The script reads
them off the frame, makes the record the select's option and value, and closes the dialog.

The record is written by the create route the create page already posts to: its CSRF check,
policy, tenancy, field access, rules, unique probe, related forms and hooks are those of a create
from the list, and a refused save renders inside the frame as it would on its own page. Pick mode
changes what surrounds the page, a bare document keeping only its toasts, and where a save lands;
on the create page, Cancel closes the dialog and Create another is gone. Closing the dialog drops
the framed form's unsaved-changes mark: the dialog's close is the user's own. The frame is a same-origin document,
which the panel's default `frame-ancestors 'self'` already admits.

The choice offers only what the related resource would show it: a record the user may create but
its scoped query leaves out is created and not chosen, and the submission of the outer form checks the key as for
any relationship choice. A creatable choice is a single, independent select over a resource's
records; the mount refuses the rest, since radios, checkboxes, a dependent choice's parent scope
and a repeater's template rows would each need their own way to take the new record, and a
condition-disabled choice would not post it.

This extends [0026](0026-topcoat-runtime-only.md), which ships no dialog script: Topcoat's runtime
has no word for reading a framed document, so the dialog is driven by `choice_create.js`, as the
option search is by `selects.js`. Without JavaScript the link opens the create page in a new tab.

## Rejected

- A create route of its own answering an `<option>`: a second create path beside the create
  page's, which the next check added to one would miss.
- Fetching the create form's markup into the dialog: markup inserted by a script loses the
  runtime's signals, so conditions, searchable choices, uploads and a refused save's re-render
  would each need reimplementing.
- Rendering the related form inside the outer page's dialog: forms cannot nest, and the related
  form's own creatable choices would render dialogs inside dialogs.
- Posting the record back with `postMessage`: the landing page would need a script of its own, and
  the frame is same-origin, so the parent can read it.
