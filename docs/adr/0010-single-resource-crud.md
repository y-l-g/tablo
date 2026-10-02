# Single-resource CRUD — Create/Edit, record fns, Policy, Notification

Date: 2026-08-31 — Status: accepted

## Decision

A `Resource` on a `Panel` is writable on the framework seams.

- **Panel** owns the resource routes — list at `{prefix}/{slug}`, `GET`/`POST` create and edit,
  `POST` delete and bulk-delete, `GET` export and options — plus the shell notification stack.
  `Router::builder().discover().cookies()` installs cookies; `Panel::layout_shell` renders the
  document.
- **Mutations** are `Resource` record fns (`form`, `validate_record`, `create_record`,
  `update_record` beside `type Form`, registered with `Panel::resource`) inside a framework-owned
  transaction, re-checking policy on the tenant-scoped snapshot; bulk delete re-fetches every ID
  and is all-or-nothing (ADR-0004). The list wires `DeleteAny` to row Delete plus bulk bar, a
  record form to the Edit link, and a non-empty `view(dx)` schema to the View link.
- **Schema** hydrates and dehydrates through typed lenses:
  `Field::text(User::fields().name()).required().email().unique()` rejects a bad field at compile
  time; the edit form hydrates from `RecordForm::hydrate`, and validation collects
  `required`/`email` inline. Unique checks app-side until Toasty exposes a violation signal, so a
  concurrent collision still surfaces as a 500. `Field::unique()` implies presence: an empty unique
  field reports `"<Label> is required"`, `.optional()` does not lift it, and mounting refuses
  `.unique()` on a column with no unique index.
- **Notification** is a transient status plus title (`success`/`error`, ~4s) rendered in the
  shell stack so it survives table swaps. It travels as a one-time `__Host-tablo_notification`
  flash cookie on a `303 See Other` redirect, so following the redirect consumes it. A page also
  mounts one in place (`notification::live_toast` plus `live_toaster`).
- **Table** parses `?q=`/`?sort=`/`?dir=`/`?after=`/`?before=` into `TableState` once.
  `Table::order_bys_for(state)` falls back to the primary key for deterministic cursors, and
  `Table::apply_declaration` turns search, filters, and ordering into a query for both loaders.
  The live shard is the slug-dispatched `table_search` behind `Table::live_search`. Row identity in
  `view!` loops is loop-level `#[key(...)]` from the row key, never the index.
