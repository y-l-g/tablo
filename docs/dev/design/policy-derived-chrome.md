# Policy-derived row chrome

Closes #383.

Line citations refer to the tree this design was written against, `105ccbb5`.

## Summary

The panel deletes `Resource::editable` and `Resource::deletable`. Every list the
panel serves carries the row policy and the delete prefix, which also enables
bulk delete; the edit prefix is attached where the registration owns the edit
routes, and the view prefix where the resource declares a `view()` schema. The
policy alone decides which links and checkboxes a row renders, so a resource
that returns `true` from `can_delete` renders delete chrome without a second
declaration.

## Motivation

A resource declares its row chrome twice: `can_update` / `can_delete` authorize
the handlers (both default-deny, `crates/tablo-core/src/resource/mod.rs:121-132`
), and `deletable()` / `editable()` decide whether the chrome renders (both
default `false`, `:146-148`, `:154-156`). The handlers re-check `can_*` and
never read the flags (`panel/actions/delete.rs`, `panel/actions/bulk.rs`,
`panel/forms/submit.rs`), so the flags authorize nothing; they only suppress
chrome. The failure mode is silent: a resource that overrides `can_delete` and
leaves `deletable()` at its default renders no Delete link and no bulk checkbox,
and nothing reports the disagreement.

`editable()` carries a build contract as well: `Panel::build` refuses a resource
registered with `Panel::resource` when `editable()` is on, because that
registration serves no edit route
(`crates/tablo-core/src/panel/build.rs:410-423`).

The showcase overrides both flags to `true` in all four of its resources
(`examples/showcase/src/app.rs:115-120`, `:266-271`, `:507-512`, `:863-868`).

## User-facing API

```rust
impl Resource for UserResource {
    type Model = User;

    fn can_view(_cx: &Cx, _record: &User) -> bool { true }
    fn can_update(_cx: &Cx, record: &User) -> bool {
        record.name != "Ken Thompson"
    }
    fn can_delete(_cx: &Cx, record: &User) -> bool {
        record.name != "Ken Thompson"
    }
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
link, a row `can_delete` allows renders the Delete control and the bulk
checkbox, and a row `can_view` refuses renders no link.

### Removed capability

`editable()` and `deletable()` were the only per-resource way to withhold chrome
the policy allows. The panel overwrites the row policy a resource sets in
`table()` (`crates/tablo-core/src/panel/list.rs:83-93`), and the wiring is
crate-private (`with_delete`, `with_edit`, `with_view`, `with_bulk_delete`,
`row_actions`, `resource/table/mod.rs:547`, `:561`, `:576`, `:587`, `:301`), so
no per-resource opt-out remains. Two replacements cover the cases an app had:

- **Deny the predicate.** `can_delete` / `can_update` returning `false` removes
  the chrome, and with it the permission the app's own `delete_record` /
  `update_record` callers had. This is the only way to keep a resource's list
  inside the panel without chrome for the action.
- **Own the route.** A page that renders its own list picks the seam:
  `panel::wired_table` carries the panel's chrome for a list-only registration
  and `panel::wired_table_with_form` for a resource registered with
  `Panel::form_resource`; a bare `Table::new(key, columns)` carries none, so the
  page draws its own links (`Table::load`, `resource/table/mod.rs:752`;
  `Table::render_with_state`, `resource/table/render/core.rs:124`). Neither is a
  per-action opt-out: the page-owned seam reproduces the panel's chrome, and the
  bare table drops all of it.

No resource in this repository sets a flag to `false` while its predicate allows
the action, so no in-tree app is affected.

## Behavior

### Chrome attachment

`wire_table_actions` (`crates/tablo-core/src/panel/list.rs:76-106`) attaches the
row policy (`:83-93`) and the chrome prefixes to every list it serves:

- Delete and bulk delete attach to every resource (`:96-99`).
- Edit attaches only where the registration serves forms, the only registration
  that pushes the edit routes (`crates/tablo-core/src/panel/mod.rs:319-330`).
  The list already carries that fact as its `FORMS` const (`panel/list.rs:230`,
  `:300`), so `wire_table_actions` takes the same flag. The live-search shard
  is registered from `finish_registration` (`panel/mod.rs:410-419`), which both
  registration paths call (`:279`, `:339`), so `search_handler_for`
  (`panel/search.rs:65`) threads the flag to the shard's call (`:75`).
- View attaches only where the resource declares a `view()` schema (`R::viewed`,
  `panel/list.rs:44-52`, `resource/mod.rs:209-211`).

The public page-owned seam splits with the same fact: `panel::wired_table`
(`panel/mod.rs:65-84`) wires a list-only resource, and `wired_table_with_form`
wires one whose registration serves forms.

Delete, bulk delete, and export routes register for every resource
(`panel/mod.rs:346-406`, pushes at `:385`, `:392`, `:400`), so a route's
presence says nothing about the chrome a resource declares.

### Per-row rendering

Per-row rendering does not change. The row policy decides once per row
(`crates/tablo-core/src/resource/table/render/core.rs:553-566`): `can_view`
decides the View link, `can_view` + `can_update` the Edit link, and `can_view` +
`can_delete` the Delete control and the bulk checkbox. A denied action emits no
URL (`:571-582`); a delete-denied row is not selectable (`:601`), and its bulk
cell stays empty so the row keeps its shape (`:353-370`).

### Column presence

Column presence does not follow the loaded page. The Actions column renders when
the table carries at least one row prefix
(`resource/table/render/core.rs:775-778`); the bulk column, its header checkbox,
and the bulk bar render with the delete prefix (`:1056-1068`,
`render/toolbar.rs:65`). The panel attaches the delete prefix to every list, so
every panel list renders both columns, and a page whose rows all fail `can_view`
renders them empty: no links, no checkboxes, and a bulk dialog that answers the
empty selection.

Per-page hiding is rejected under Alternatives: the streamed skeleton renders
the real `<thead>` before any page exists (`render/skeleton.rs:43-46`) and its
column set is pinned to the table that replaces it (`:180-184`, `:216-225`), and
the fixed layout derives column widths from the chrome set
(`render/core.rs:833-845`) so that a filter or a page change cannot re-measure
the table (`render/skeleton.rs:144-147`).

### Errors

- A resource that keeps `fn editable()` or `fn deletable()` fails to compile:
  neither is a trait member (E0407).
- The row key and the columns are constructor arguments since #384
  (`Table::new(key, columns)`, `resource/table/mod.rs:210`, and
  `Resource::table` is required, `resource/mod.rs:399`), so the build-time
  missing-key error is gone: `Panel::build` refuses only `paginate(0)`
  (`resource/table/mod.rs:882-887`, reached from `panel/build.rs:398`). Every
  panel list carries delete chrome, so every resource still declares its row key
  — in the constructor.
- A hand-crafted POST against an action the page does not render still answers
  403: the handlers keep their own `can_view` + `can_update` / `can_delete`
  checks (`panel/actions/delete.rs:72-77`, `panel/actions/bulk.rs:101-106`,
  `panel/forms/submit.rs:325-330`, `:344-349`).

## Edge cases

- **Fully denied page.** The list checks `can_view_any` for membership and does
  not filter rows by `can_view` (`resource/mod.rs:84`, `:104-132`), so such a
  page still lists its rows and renders the action columns with empty cells; the
  next page renders the links. No column appears or disappears.
- **Tables outside the panel list.** A table that carries no prefix consults no
  policy and renders `RowActions::ALL`
  (`resource/table/render/core.rs:553-566`). A relation table carries no prefix;
  a page-owned list takes the panel's chrome from `panel::wired_table` /
  `wired_table_with_form`; a bare `Table::new` carries none. Every panel list
  attaches the delete prefix, so it consults the policy once per row; the
  predicates are synchronous in-memory functions over `&Cx` and the record
  (`resource/mod.rs:104-132`).
- **Live search.** A shard re-render keeps the columns the streamed list
  rendered, because both derive them from the same prefixes and the same forms
  flag.
- **Exports and relationship option loads** keep their per-row `can_view` gating
  (`panel/actions/export.rs:133`, `:199`, `schema/relationship.rs:321`).

## Alternatives

- **Page-driven column visibility.** Hide the Actions and bulk columns when no
  loaded row allows them. Rejected: the skeleton has no rows to evaluate and its
  column set is pinned to the swapped table's (`render/skeleton.rs:180-184`,
  `:216-225`); the bulk column carries the selection transport
  (`render/toolbar.rs:85-102`), so a live shard could drop a selection the
  handler then refuses; and column widths derive from the chrome set
  (`render/core.rs:833-845`). The price of the rejection is the empty columns on
  an all-denied page described under Edge cases.
- **Default both flags to `true`.** Fixes the forgotten override and keeps the
  read-only recipe. Rejected: it keeps two declarations of one intent. The
  predicate says who is allowed to act and the flag says whether the panel
  offers the action, and nothing keeps the two consistent, which is the
  duplication this design removes. Its price is the capability described under
  "Removed capability".
- **Keep one whole-resource chrome switch.** Rejected for the same reason, and
  no resource in the tree needs it.
- **Restore a public per-action wiring seam.** Rejected: #384 made `with_delete`
  / `with_edit` / `with_view` / `with_bulk_delete` and `row_actions`
  crate-private (`resource/table/mod.rs:547`, `:561`, `:576`, `:587`, `:301`) so
  the panel's wiring has one production caller, and `panel::wired_table` is the
  supported way to render a list outside the panel. Making the prefixes public
  again reopens what that seal closes, for a capability no resource in the tree
  uses.

## Implementation plan

- Delete `Resource::editable` / `deletable` (`resource/mod.rs:146-156`) and the
  `editable()` arm of `check_list_resource` (`panel/build.rs:410-423`).
- `wire_table_actions` stops reading `declared_chrome`, takes the forms flag,
  attaches delete and bulk delete unconditionally, attaches edit from the flag,
  and view from `R::viewed` (`panel/list.rs:76-106`). Thread the flag through
  `finish_registration` / `search_handler_for` (`panel/mod.rs:410-419`,
  `panel/search.rs:65`) and through both `resource_list` monomorphizations
  (`panel/list.rs:230`, `:300`).
- `TableChrome` and `declared_chrome` reduce to the view flag
  (`resource/table/mod.rs:81-97`, `panel/list.rs:41-52`).
- Split the page-owned seam into `wired_table` and `wired_table_with_form`
  (`panel/mod.rs:65-84`), and update the rustdocs that name the deleted flags
  (`panel/mod.rs:65-81`, `resource/table/mod.rs:81-87`).
- Update the fixtures that override the flags (`panel/list.rs:523`, `:917`,
  `:990`, `:1155-1162`, `:1198-1201`, `:1351-1354`; `panel/build.rs:1218-1221`;
  `resource/table/mod.rs:1053`) and the showcase overrides
  (`examples/showcase/src/app.rs:115-120`, `:266-271`, `:507-512`, `:863-868`).
- Land the implementation as a breaking commit: `!` after the type, with a
  `BREAKING CHANGE:` footer naming `Resource::editable`, `Resource::deletable`,
  and the `wired_table` / `wired_table_with_form` split
  (`docs/dev/COMMITS.md:43-53`).

## Docs

- `CONTEXT.md:167-193`: delete the `editable` (`:178`) and `deletable` (`:188`)
  entries and update the Policy entry (`:167-176`). The same file says a refused
  row renders a disabled bulk checkbox (`:192-193`); the code renders an empty
  cell (`render/core.rs:353-370`). Correct that in the same pass.
- `docs/guide/src/resources.md:37`, `:45-49` (the opt-in rule and the build
  refusal) and `docs/guide/src/tables.md:25-31`, `:104-109` (the `Table::key`
  requirement and the bulk column).
- `docs/dev/architecture.md:90`, `docs/adr/0010-single-resource-crud.md:18-20`,
  and the rustdoc at `panel/list.rs:41-44`, `:60-75`, and
  `resource/table/mod.rs:81-87`.

## Open questions

- Blocking-acceptance: accept that no per-action opt-out remains. A resource
  cannot allow an action and withhold its chrome inside the panel; the two
  replacements are denying the predicate and owning the route.
- Blocking-implementation: the `wired_table` / `wired_table_with_form` split and
  the forms flag threaded through `finish_registration`, `search_handler_for`,
  and `wire_table_actions`.
- Deferrable: a whole-resource probe beside `can_view_any` (`can_delete_any` /
  `can_update_any`) would hide a column when no row allows the action. It is
  evaluated once per request, so the skeleton and the table agree; unlike the
  page-driven rule rejected above, it does not read the loaded page. No resource
  in the tree needs it.

## Out of scope

`can_*` signatures and their default-deny behavior. The detail page,
`view_relations` gating, and tenancy scoping. Bulk actions other than delete:
none exist (`resource/table/mod.rs:587-594`). #384's constructor change is
context, not part of this design.
