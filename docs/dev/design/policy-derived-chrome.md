# Policy-derived row chrome

Closes #383.

Line citations refer to the tree this design was written against, `f9750974`.

## Summary

`Resource::editable` and `Resource::deletable` are deleted. The panel wires
`can_view` / `can_update` / `can_delete` into every table's row policy, and the
row policy alone decides which links and checkboxes a row renders. A column
renders when at least one row on the loaded page allows the action behind it.

## Motivation

A resource declares row chrome twice: `can_update` / `can_delete` authorize the
handlers (both default-deny, `crates/tablo-core/src/resource/mod.rs:121-132`),
and `editable()` / `deletable()` toggle the chrome (both default `false`,
`:146-156`). The flags grant nothing: the delete, bulk delete, and edit
handlers re-check `can_*` and never read the flags (zero flag reads in
`panel/actions/delete.rs`, `panel/actions/bulk.rs`, `panel/forms/submit.rs`),
while a predicate-on/flag-off resource renders no chrome. The showcase repeats
all 8 overrides returning `true` (`examples/showcase/src/app.rs:115-120`,
`:265-270`, `:505-510`, `:860-865`).

## User-facing API

```rust
impl Resource for UserResource {
    type Model = User;

    fn can_view(_cx: &Cx, _record: &User) -> bool { true }
    fn can_update(_cx: &Cx, record: &User) -> bool { record.name != "Ken Thompson" }
    fn can_delete(_cx: &Cx, record: &User) -> bool { record.name != "Ken Thompson" }
    // no editable() / deletable()
}
```

Before and after: delete the two flag overrides; keep the predicates. A denied
row renders no link and an empty bulk cell (the cell stays for shape); a fully
denied page renders no Actions column and no bulk column.

## Behavior

`wire_table_actions` (`crates/tablo-core/src/panel/list.rs:78-109`) attaches the
row policy (`:86-95`) and all chrome prefixes unconditionally; the flag gates
(`:96-107`) are deleted. Delete, bulk delete, and export routes register for
every resource through `register_common`
(`crates/tablo-core/src/panel/mod.rs:355-380`), so prefix presence no longer
encodes authorization. Per-row rendering is unchanged: the renderer consults
`actions_for` per row (`resource/table/render/core.rs:587-591`), a denied edit,
view, or delete emits no URL (`:597-618`), and a delete-denied row is not
selectable (`:627`, empty cell at `:379-395`). Column rendering changes from
flag-driven to page-driven: the Actions column renders when any loaded row
allows view, edit, or delete; the bulk column, its header checkbox
(`:1082-1094`), and the bulk bar (`render/toolbar.rs:70-72`) render when any
loaded row is selectable. An empty page renders neither column. Export and
relationship option loads keep their per-row `can_*` gating.

## Edge cases

- **Fully denied page.** A page whose rows all deny renders no action columns;
  another page of the same table can render them. Column visibility follows the
  loaded page, never a cached whole-table answer.
- **Chromeless tables.** A table with no prefixes never consults the policy
  (`render/core.rs:578-591`), so removing the flags adds no per-record calls
  there.
- **Hand-crafted POSTs.** Handler checks are untouched
  (`panel/actions/delete.rs:72-75`, `panel/actions/bulk.rs:101-104`,
  `panel/forms/submit.rs:325-347`), so a POST against a hidden action still
  answers 403.

## Alternatives

- **Default both flags to `true`.** Keeps two switches for one promise:
  flag-off/predicate-on renders nothing and flag-on/predicate-off renders
  chrome that 403s. Discarded.
- **Whole-table pre-query for column visibility.** An extra query per list render
  to decide columns before loading rows. The rows are already in hand at render
  time, so the page-driven rule needs no query. Discarded.

## Open questions

- Blocking-acceptance: confirm hiding (rather than disabling) the action and
  bulk columns on fully denied or empty pages.
- Blocking-implementation: none.
- Deferrable: none.

## Out of scope

Policy vocabulary (`can_*` signatures and default-deny) is unchanged. The
detail page, `view_relations` gating, and tenancy scoping are unchanged.
