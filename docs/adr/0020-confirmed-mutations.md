# Confirmed mutations: the response is a page, the table is re-run by its shard

Date: 2026-09-23 — Status: accepted

## Decision

**A confirmed delete stays a POST that 303s to the list.** Handlers, policy and confirmation
checks, flash notification, and the no-JS path are unchanged (ADR-0004, ADR-0010). A form marked
`data-mutation-submit` — row-delete and bulk confirms — is posted by `mutation-submit.js` with
`fetch` and applied in place. Without JavaScript the marker is inert and the form POSTs and 303s.
The bulk form carries bulk custom actions as submit buttons with their own `formaction`; the
script posts to it when present, else to the form action. A row custom action is a plain form
that POSTs and 303s.

**The mutation response is the whole list page; the client reads two things.** The toast surfaces
(`[data-sonner-toast]`) are inserted into the shell toaster, never morphed. The table is not taken
from the response on a live table.

**The table refreshes through its own shard.** A live table renders a hidden
`[data-table-revision]` input declared by `Table::render_inner`; writing it changes a signal the
`table_search` shard tracks, so the runtime re-fetches and morphs the region for the current
query. The signal lives in the shard content scope, stable across reruns and distinct per shard.
The client bumps a monotonic counter. A static table region is replaced wholesale: with no
signals there is no binding or shard.

**The client finds the table from the form.** The bulk form lives inside its
`[data-table-root]`; the row confirm lives in the page dialog, so its table is the one holding
the control that opened it. No matching control leaves the page to the browser.

**The client never morphs the response into a live document.** The response renders the bare list
URL while live query state lives in signals and the page-owned toolbar outside
`[data-boundary="table"]`, which the client cannot read or reset. A morph would show page 1
unfiltered under a stale toolbar until the next rerun.

**Failure paths stay the browser's.** A self-answered POST (4xx/5xx) wrote nothing, so the client
returns the form with `form.submit()`. A request never completing and a followed redirect with a
failed list render reload instead, since posting again risks a second write.

The URL keeps table state; only dialog `delete`/`open` parameters drop. Bulk selection prunes by
exactly the removed keys. Focus moves to the row taking the deleted row's place, else the bulk
trigger; a modal dialog closes before the rerun.
