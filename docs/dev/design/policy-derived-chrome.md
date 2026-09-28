# Policy-derived row chrome

Closes #383.

Line citations refer to the tree this design was written against, `f9750974`.

## Summary

The panel deletes `Resource::editable` and `Resource::deletable`. Every list the
panel serves carries the row policy and the delete prefix, which also enables bulk
delete; the edit prefix is attached where the registration owns the edit routes. The
policy alone decides which links and checkboxes a row renders, so a resource that
returns `true` from `can_delete` renders delete chrome without a second declaration.

## Motivation

A resource declares its row chrome twice: `can_update` / `can_delete` authorize the
handlers (both default-deny, `crates/tablo-core/src/resource/mod.rs:121-132`), and
`deletable()` / `editable()` decide whether the chrome renders (both default
`false`, `:146-148`, `:154-156`). The handlers re-check `can_*` and never read the
flags (`panel/actions/delete.rs`, `panel/actions/bulk.rs`,
`panel/forms/submit.rs`), so the flags authorize nothing; they only suppress chrome.
The failure mode is silent:
a resource that overrides `can_delete` and leaves `deletable()` at its default
renders no Delete link and no bulk checkbox, and nothing reports the disagreement.

`editable()` carries a build contract as well: `Panel::build` refuses a resource
registered with `Panel::resource` when `editable()` is on, because that registration
serves no edit route (`crates/tablo-core/src/panel/build.rs:411-425`).

The showcase overrides both flags to `true` in all four of its resources
(`examples/showcase/src/app.rs:115-120`, `:265-270`, `:505-510`, `:860-865`).

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

### Before and after

Before, the same resource declared its chrome a second time:

```rust
fn editable() -> bool { true }
fn deletable() -> bool { true }
```

After, the resource deletes both overrides and keeps the predicates. The panel
derives the chrome from them per row: a row `can_update` allows renders the Edit
link, a row `can_delete` allows renders the Delete control and the bulk checkbox,
and a row `can_view` refuses renders no link.

### Removed capability

`editable()` and `deletable()` were the only per-resource way to withhold chrome the
policy allows. The panel overwrites the row policy a resource sets in `table()`
(`crates/tablo-core/src/panel/list.rs:79`, then `:86-95`), so no per-resource
opt-out remains. An app that relied on a flag set to `false` with the predicate on
has two options:

- Deny the predicate (`can_delete` / `can_update` returning `false`). This also
  removes the permission the app's own `delete_record` / `update_record` callers
  had.
- Render that list outside the panel, on a route the app owns, using the public
  table API (`Table::with_delete` / `with_edit` / `with_view`,
  `crates/tablo-core/src/resource/table/mod.rs:573-615`, then `Table::load`,
  `:777`, and `Table::render_with_state`,
  `resource/table/render/core.rs:124`). That keeps `Table::with_*` as the chrome
  switch, and costs the app the route, the auth gate, and the table state the panel
  would otherwise provide.

No resource in this repository sets a flag to `false` while its predicate allows the
action, so no in-tree app is affected.

## Behavior

### Chrome attachment

`wire_table_actions` (`crates/tablo-core/src/panel/list.rs:78-109`) attaches the row
policy (`:86-95`) and the chrome prefixes to every list it serves:

- Delete and bulk delete attach to every resource.
- Edit attaches only where the resource is registered with `Panel::form_resource`,
  the only registration that pushes the edit routes
  (`crates/tablo-core/src/panel/mod.rs:296-307`). The list already carries whether
  the resource serves forms (`FORMS`, `panel/list.rs:232`, `:267`, `:302`), and
  `finish_registration` registers the live-search handler from both registration
  paths (`panel/mod.rs:388-393`), so it takes the same flag and the shard attaches
  the same prefixes.
- View attaches only where the resource declares a `view()` schema
  (`R::viewed`, `panel/list.rs:50-52`, `resource/mod.rs:209-211`).

Delete, bulk delete, and export routes register for every resource
(`panel/mod.rs:324-384`, pushes at `:361-382`), so a route's presence says nothing
about the chrome a resource declares.

### Per-row rendering

Per-row rendering does not change. The row policy decides once per row
(`crates/tablo-core/src/resource/table/render/core.rs:587-591`): `can_view` decides
the View link, `can_view` + `can_update` the Edit link, and `can_view` +
`can_delete` the Delete control and the bulk checkbox. A denied action emits no URL
(`:597-618`); a delete-denied row renders no checkbox, and the cell stays so the row
keeps its shape (`:627`, `:379-395`).

### Column presence

Column presence does not follow the loaded page. The Actions column renders when the
table carries at least one row prefix (`resource/table/render/core.rs:802-804`); the
bulk column, its header checkbox, and the bulk bar render with the delete prefix
(`:1082-1094`, `render/toolbar.rs:65-72`). The panel attaches the delete prefix to
every list, so every panel list renders both columns, and a page whose rows all fail
`can_view` renders them empty: no links, no checkboxes, and a bulk dialog that
answers the empty selection.

Per-page hiding is rejected under Alternatives: the streamed skeleton renders the
real `<thead>` before any page exists (`render/skeleton.rs:43-50`) and its column
set is pinned to the table that replaces it (`:180-184`, `:216-225`), and the fixed
layout derives column widths from the chrome set (`render/core.rs:859-937`) so that
a filter or a page change cannot re-measure the table (`:345-347`).

### Errors

- A resource that keeps `fn editable()` or `fn deletable()` fails to compile:
  neither is a trait member (E0407).
- `Panel::build` refuses a table that renders action chrome without a `Table::key`
  record key (`resource/table/mod.rs:928-932`, reached from
  `panel/build.rs:395-405`). Every panel list now carries delete chrome, so every
  resource registered with the panel needs `Table::key`. A table that declares no
  chrome and no record key stops building (`panel/build.rs:1350-1353`).
- A hand-crafted POST against an action the page does not render still answers 403:
  the handlers keep their own `can_view` + `can_update` / `can_delete` checks
  (`panel/actions/delete.rs:72-77`, `panel/actions/bulk.rs:101-106`,
  `panel/forms/submit.rs:325-330`, `:344-349`).

## Edge cases

- **Fully denied page.** The list checks `can_view_any` for membership and does not
  filter rows by `can_view` (`resource/mod.rs:96-103`), so such a page still lists
  its rows and renders the action columns with empty cells; the next page renders
  the links. No column appears or disappears.
- **Tables outside the panel list.** A relation table, or a table an app renders
  itself, carries no prefix and consults no policy
  (`resource/table/render/core.rs:578-580`). Every panel list attaches both
  prefixes, so it consults the policy once per row; the predicates are synchronous
  in-memory functions over `&Cx` and the record (`resource/mod.rs:104-132`), and a
  list that attaches no prefix consults nothing.
- **Live search.** A shard re-render keeps the columns the streamed list rendered,
  because both derive them from the same prefixes.
- **Exports and relationship option loads** keep their per-row `can_view` gating
  (`panel/actions/export.rs:133`, `:199`, `schema/relationship.rs:321`).

## Alternatives

- **Page-driven column visibility.** Hide the Actions and bulk columns when no
  loaded row allows them. Rejected: the skeleton has no rows to evaluate and its
  column set is pinned to the swapped table's (`render/skeleton.rs:180-184`,
  `:216-225`); the bulk column carries the selection transport
  (`render/toolbar.rs:85-102`), so a live shard could drop a selection the handler
  then refuses; and column widths derive from the chrome set
  (`render/core.rs:859-937`). The price of the rejection is the empty columns on an
  all-denied page described under Edge cases.
- **Default both flags to `true`.** Fixes the forgotten override and keeps the
  read-only recipe. Rejected: it keeps two declarations of one intent. The predicate
  says who is allowed to act and the flag says whether the panel offers the action,
  and nothing keeps the two consistent, which is the duplication this design
  removes. Its price is the capability described under "Removed capability".
- **Keep one whole-resource chrome switch.** Rejected for the same reason, and no
  resource in the tree needs it.

## Implementation plan

- Delete `Resource::editable` / `deletable` (`resource/mod.rs:146-156`) and the
  `editable()` arm of `check_list_resource` (`panel/build.rs:411-425`).
- `wire_table_actions` stops reading `declared_chrome`, attaches delete and bulk
  delete unconditionally, and attaches edit when the registration serves forms
  (`panel/list.rs:78-109`, `:232`; `panel/mod.rs:388-393`; `panel/search.rs:75`).
- `TableChrome` and `declared_chrome` reduce to the view flag, and
  `missing_essentials` requires the record key without a chrome argument
  (`resource/table/mod.rs:91-108`, `:928-932`; `panel/list.rs:46-54`).
- Update the test fixtures that override or assert the flags
  (`panel/list.rs:918-933`, `:991-1002`, `:1157-1188`, `:1359-1362`;
  `panel/build.rs:1229-1245`, `:1350-1353`) and the showcase overrides.
- Migrate the detached harness, which overrides both flags and re-derives the
  wiring by hand (`benchmarks/tablo/src/main.rs:101-116`, `:288-299`).
- Land the implementation as a breaking commit: `!` after the type, with a
  `BREAKING CHANGE:` footer naming `Resource::editable` and `Resource::deletable`
  (`docs/dev/COMMITS.md:43-53`).

## Docs

- `CONTEXT.md:180-199`: delete the `editable` and `deletable` entries and update the
  Policy entry at `:168-176`. The same file says a refused row renders a disabled
  bulk checkbox (`:195-196`); the code renders an empty cell
  (`render/core.rs:379-395`). Correct that in the same pass.
- `docs/guide/src/resources.md:35-60` (the opt-in rule and the build refusal) and
  `docs/guide/src/tables.md:24-32`, `:104-112` (the `Table::key` requirement and the
  bulk column).
- `docs/dev/architecture.md:89-90`, `docs/adr/0010-single-resource-crud.md:17-20`,
  and the rustdoc at `panel/list.rs:62-70` and `resource/table/mod.rs:84-89`.

## Open questions

None blocking. Settled before this document: static columns over per-page hiding,
deleting the flags over defaulting them to `true`, and requiring `Table::key` of
every panel resource. Deferrable: a whole-resource probe beside `can_view_any` would
let a resource hide delete chrome for its whole list without a flag; no resource in
the tree needs it.

## Out of scope

`can_*` signatures and their default-deny behavior. The detail page,
`view_relations` gating, and tenancy scoping. Bulk actions other than delete: none
exist (`resource/table/mod.rs:615-624`).
