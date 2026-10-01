# Single-resource CRUD — Create/Edit, record fns, Policy, Notification

Date: 2026-08-31 — Status: accepted — Amended: 2026-09-10, 2026-09-15, 2026-09-21, 2026-09-22, 2026-09-28, 2026-09-29, 2026-10-01

## Decision

A `Resource` registered on a `Panel` is fully writable on the framework's existing seams.

- **Panel** owns the resource routes — the list page at `{prefix}/{slug}`, `GET`/`POST`
  `{prefix}/{slug}/create`, `GET`/`POST` `{prefix}/{slug}/{id}/edit`, `POST`
  `{prefix}/{slug}/{id}/delete`, `POST` `{prefix}/{slug}/bulk-delete`, `GET`
  `{prefix}/{slug}/export` and `GET` `{prefix}/{slug}/options` — plus the shell-level notification
  stack. `Router::builder().discover().cookies()` installs the cookie layer, and
  `Panel::layout_shell` renders the complete document.
- **Mutations** are `Resource` record fns called by the handlers inside a framework-owned
  transaction, with `can_*` re-checked on the tenant-scoped loaded snapshot; a bulk delete re-fetches
  every id through the same query and is all-or-nothing (ADR-0004). The panel's list loader wires the
  chrome the resource's declarations imply (`Resource::can_delete_any` → row Delete + bulk bar, a
  record form → Edit link, `Resource::viewed` → View link) as the table's delete, edit and view
  action prefixes (GH #383, GH #384); the delete handlers check `can_delete_any` too.
- **Schema** hydrates and dehydrates through typed lenses:
  `Field::text(User::fields().name()).required().email().unique()` fails to compile on a bad
  field; `Schema::hydrate` fills `value` attrs from the Model through `Resource::hydrate_form_values`,
  and validation collects `required`/`email` inline per field in the reserved destructive slot. Unique
  is checked app-side until Toasty exposes a unique-violation signal, so a concurrent write that
  violates the index still surfaces as a 500 (GH #88, open). `Field::unique()` implies **presence**
  (GH #189): an empty unique field reports `"<Label> is required"` inline, `.optional()` does not lift
  it, and mounting the panel refuses a `.unique()` marker on a column with no unique index.
- **Notification** is a transient status + title (`success`/`error`, ~4s) produced by a mutation's
  result and rendered in the shell's top-level stack so it survives table swaps. It travels as a
  one-time flash cookie on a `303 See Other` redirect — `__Host-tablo_notification`, carrying
  `Secure` like the session and CSRF cookies, holding Topcoat's `CookieStore` JSON — so following the
  redirect consumes it and a reload never replays it. A page can also mount one in place
  (`notification::live_toast` plus the shell's `live_toaster`).
- **Table** parses `?q=`/`?sort=`/`?dir=`/`?after=`/`?before=` into `TableState` once. The declared
  default ordering and `?sort=` resolution are one entry point,
  `Table::order_bys_for(state)`, which falls back to the primary key so cursor pagination has a
  deterministic order, and
  `Table::apply_declaration` is the one routine that turns search, filters and ordering into a query
  for both loaders. The live shard is the slug-dispatched `table_search` behind `Table::live_search`,
  fed by the page's `TableSignals`. Row identity in `view!` loops is a loop-level `#[key(...)]` taken
  from the row key, never the loop index (GH #124).

## Consequences

`{prefix}/{slug}` supports search/sort/paginate/create/edit/delete/bulk-delete, all policy-checked,
with no N+1. `Panel` remains the single owner of Router/Db/Shell; `Resource::query` is the resource's
own row-scoping seam with the framework applying the tenant half (ADR-0002); `Schema` stays the form
seam and `Table` the list seam. No `Resource` hand-rolls `#[shard]`. The showcase test modules cover
the vertical slice, and `cargo test --workspace` / `clippy -D warnings` / `fmt` stay green per commit.

## Amendment — 2026-09-28

**Hydration and the create/update record fns belong to the record form.** The decision's
`Resource::hydrate_form_values` is superseded by this amendment: the edit form hydrates from
`RecordForm::hydrate`, and `form`, `validate_record`, `create_record`, and `update_record` live on
`Resource` beside its `type Form`, registered with `Panel::resource` (ADR-0022).

## Amendment — 2026-10-01

**The View link follows the declared view, not a `Resource::viewed`.** The decision's
`Resource::viewed` no longer exists: whether a resource declares a detail page is the cached
declarations' `viewed()` — a non-empty `view(dx)` schema — which the panel wires as the table's
view prefix. A resource with no view renders no link instead of one that 404s.
