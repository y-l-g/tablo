# Policy-derived row chrome

Closes #383. Depends on [`single-resource-registration.md`](single-resource-registration.md)
(#382): the Edit chrome reads `RecordForm::HAS_FORM`, which #382 adds. Implement #382 first.

Citations refer to `36f9382c`.

## Summary

`Resource::editable` and `Resource::deletable` are deleted. The panel derives each row action's
chrome from a declaration that also governs the route:

| Chrome | Attached when | Per-row gate (unchanged) |
| --- | --- | --- |
| View link | `R::viewed(cx)` | `can_view` |
| Edit link | `R::Form::HAS_FORM` | `can_view && can_update` |
| Delete link, bulk column, bulk bar | `R::can_delete_any(cx)` (**new**) | `can_view && can_delete` |

## Motivation

The flags authorize nothing: the delete, bulk, and edit handlers check `can_*` and never read them
(`panel/actions/delete.rs:72-77`, `panel/actions/bulk.rs:101-106`, `panel/forms/submit.rs`). A
resource that overrides `can_delete` and leaves `deletable()` at `false` renders no Delete control,
and nothing reports it. Every resource in the tree (four in the showcase, one in the benchmark) sets
both flags to `true` next to the predicates they duplicate.

Attaching delete chrome to every list instead would render an empty checkbox column, a select-all
checkbox, and a bulk bar on every read-only resource (`render/core.rs:353-370`, `:1056-1068`,
`render/toolbar.rs`). The column set cannot follow the loaded rows: the skeleton renders `<thead>`
before any row exists (`render/skeleton.rs:43-46`) and pins its columns to the table it streams in.
A whole-resource predicate, answered once per request, is what decides the column.

## API

```rust
impl Resource for CommentResource {
    // Anyone may delete any row: one method.
    fn can_delete_any(_cx: &Cx) -> bool { true }
}

impl Resource for UserResource {
    fn can_delete_any(_cx: &Cx) -> bool { true }
    // Per-row refinement on top.
    fn can_delete(_cx: &Cx, record: &User) -> bool { record.name != "Ken Thompson" }
}
```

### `Resource` changes

| Item | Change |
| --- | --- |
| `fn can_delete_any(cx: &Cx) -> bool` | new, default `false` |
| `fn can_delete(cx: &Cx, record: &Self::Model) -> bool` | default becomes `Self::can_delete_any(cx)` |
| `fn editable()`, `fn deletable()` | deleted; an override fails with E0407 |

`can_update` gets no `_any` counterpart: the Edit link's presence follows `HAS_FORM`, and a denied
row renders no link inside the Actions column the View or Delete chrome already needs.

## Behavior

**Wiring.** `declared_chrome` (`panel/list.rs:41-52`) returns
`TableChrome { delete: R::can_delete_any(cx), edit: R::Form::HAS_FORM, view: R::viewed(cx) }`.
`wire_table_actions` is otherwise unchanged. It serves the streamed list, the live-search shard,
and `panel::wired_table`, so all three agree without a new parameter.

**Handlers.** The delete and bulk-delete POSTs answer 403 when `!R::can_delete_any(cx)`, checked
before the transaction opens, then keep their per-row `can_view` + `can_delete` checks. A
hand-crafted POST is refused by the same predicate that removed the chrome.

**Columns.** The Actions column renders when any row prefix is attached (`render/core.rs:775-778`).
The bulk column and bar render with the delete prefix. A read-only resource with no `view()`
schema and no form renders neither.

**Build.** `check_form_declaration` (`panel/build.rs`, from #382) drops its `editable()` arm. No new build check:
`can_delete_any` may depend on the request.

## Edge cases

- **`can_delete` overridden, `can_delete_any` left at `false`.** No delete chrome and every delete
  POST is a 403. The resource is consistently read-only; the fix is to allow `can_delete_any`.
- **`can_delete_any` true, every row denied.** The bulk column renders with empty cells, as a denied
  row does today (`render/core.rs:363-369`).
- **Tables outside the panel.** A bare `Table::new(key, columns)` carries no prefix and consults no
  policy; `panel::wired_table` carries the panel's chrome.

## Alternatives

- **Attach delete chrome to every list.** Empty columns and a bulk bar on every read-only resource.
- **Default both flags to `true`.** Keeps two unsynchronized declarations of one intent.
- **Hide columns from the loaded page.** The skeleton has no rows, and live search would drop a
  selection mid-swap.

## Implementation

1. `resource/mod.rs`: delete `editable` / `deletable` and their trait-level docs (`:58`, `:64-66`,
   `:134-156`); add `can_delete_any`; change `can_delete`'s default.
2. `panel/list.rs`: `declared_chrome` as above; rewrite the `wire_table_actions` rustdoc
   (`:54-74`).
3. `panel/actions/delete.rs`, `panel/actions/bulk.rs`: the `can_delete_any` gate.
4. Fixtures: `panel/list.rs` (`:523`, `:917`, `:990`, `:1155-1162`, `:1198-1201`, `:1351-1354`, and
   the `denied_rows_render_no_edit_chrome` rustdoc at `:1225-1231`), `panel/build.rs:1218-1221`,
   `resource/table/mod.rs:1053`. Add tests: a default resource renders no bulk column; a POST to
   delete on a `can_delete_any = false` resource answers 403.
5. `examples/showcase/src/app.rs`: replace the flag overrides (`:115-120`, `:266-271`, `:507-512`,
   `:863-868`) with `can_delete_any`, and fix the read-only recipe at `:754`;
   `examples/showcase/tests/comments_check.rs:43-45`.
6. `benchmarks/tablo/src/main.rs:102-117`: same migration; format and lint by manifest path.
7. Docs: `CONTEXT.md` (Policy entry; delete the `editable` / `deletable` entries; a refused row
   renders an empty bulk cell, not a disabled checkbox), `docs/guide/src/resources.md:37-49`,
   `docs/guide/src/tables.md:104-109`, `docs/dev/architecture.md:90`,
   `docs/adr/0010-single-resource-crud.md:18-20`, the `wired_table` rustdoc (`panel/mod.rs:65-81`),
   and the `TableChrome` rustdoc (`resource/table/mod.rs:81-87`).
8. Commit: `feat(core)!: …`, with a `BREAKING CHANGE:` footer naming `Resource::editable`,
   `Resource::deletable`, `Resource::can_delete_any`, and the new `can_delete` default.

## Out of scope

`can_view` / `can_update` / `can_create` signatures and defaults, the detail page, tenancy, and bulk
actions other than delete (none exist).
