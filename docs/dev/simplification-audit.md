# Simplification spec

Where the workspace is larger or more intricate than its feature set requires, and the change that
removes the excess. Each item states the current code, the change, and what the change removes.

Baseline: `master` at `c3c967d3`. Every item cites the file and the symbol it concerns, not a line.
Tablo is experimental: an item may break the public API, and none keeps a compatibility shim.

## 1. Target state

**Declaration.** One `Resource` trait declares a resource: its model, its record form, its list
table, its policy, and its record functions. The form's schema also renders the detail page. One
field type with a control discriminant covers text, multi-line text, choice, and file. Field
metadata is resolved once when the schema is built.

**List.** The URL query string is the only list state; the live shard and the GET path parse it with
one function. One cursor type. One loader, used by the list, the shard, the export, and the
benchmark. `Table` is a declaration plus pure planning, with no `Cx` and no `Db`. A page size cannot
be zero by construction.

**Mutation.** One write pipeline: auth, body, CSRF, transaction, scoped re-load, policy, record fn,
commit, `after_commit`. Create and update customize it through a check hook. Delete and bulk delete
share the create and update tail.

**View.** One render entry per widget, taking the state it renders. The skeleton and the loaded
table derive their chrome from one source. One `<option>` renderer.

**Infrastructure.** One authentication configuration (`Auth::disabled()` is the opt-out). One error
vocabulary. One option source. No `auth` cargo feature.

## 2. Order

Each batch is independently mergeable, ends with the gate set for what it touched, and lands in this
order. Items inside a batch are listed in the order they apply.

| Batch | Items | Why here |
| --- | --- | --- |
| 1. Tests | T1 | Mechanical, and every later diff reviews smaller. Nothing else is in flight. |
| 2. Fixes and removals | S22, S8, S16, S13, removal list, doc claims | Bugs and dead surface; no redesign. |
| 3. Mutation | S15, S9 | Small, and S9 settles the record-fn surface the declaration batch builds on. |
| 4. Declaration | S4, S3, S6, S5, S7, S1 | S4's field list is what S3, S6, and S7 build on; S1 needs S3's modifier. |
| 5. List | S12, S14, S11, S10, S2, S27 | S12 gives one loader to change; S10 and S27 then own the URL. |
| 6. View | S17, S18, S19, S28 | Renders the list state batch 5 settles. |
| 7. Infrastructure | S20, S24, S25, S26 | Independent; last because it touches every module. |
| 8. Guide | G1, G2 | Written against the API the batches above leave. |

The upstream gaps #397, #398, and #399 are not blockers. Each removes more code when it lands: #397
deletes the cursor probe in the S12 loader, #398 deletes `cursor.rs`, and #399 deletes the URL
builder S10 and S27 keep.

## 3. Items

### Batch 1 — Tests

**T1 — Move inline test modules beside their subjects.**

About half of `crates/tablo-core/src` is `#[cfg(test)] mod tests` blocks. `panel/list.rs`,
`panel/actions/export.rs`, and `panel/build.rs` are between two thirds and four fifths tests.

**Change.** `#[cfg(test)] mod tests;` in a sibling `tests.rs` per module whose tests outweigh its
code, reusing the fixtures that exist (`panel/test_support.rs`, `test_support.rs`,
`tests/common/mod.rs`). No test changes.

**Removes.** Nothing. A source file reads as its production code.

### Batch 2 — Fixes and removals

**S22 — Method-scope the login gate bypass.**

The auth gate (`auth.rs`) passes any method at `{prefix}/login` through unauthenticated, while the
logout bypass below it is scoped to POST. An app route at `{prefix}/login` under PUT or DELETE runs
unauthenticated.

**Change.** Pass through only GET and POST at the login path.

**S8 — One name per resource.**

`navigation_label()` (`resource/mod.rs`) is plural and titles singular actions:
`panel/list.rs`, `panel/forms/render.rs`, and `panel/forms/submit.rs` render `Create {label}`, and
`panel/forms/submit.rs` and `panel/forms/common.rs` render `Edit {label}`, so the showcase serves
"Create Blog Posts". `slug()` derives from the resource type and the label from the model type, so
`StaffResource` over `User` is `/staff` labelled "Users".

**Change.** Add `fn label() -> String`, singular, defaulted from the model type name. Derive
`navigation_label()` as its plural and use `label()` in the action titles. A wrong plural costs one
override, so the irregular, f-exception, and uncountable tables in `resource/naming.rs` go.

**Removes.** The wrong titles, the naming tables, and the two-source naming split.

**S16 — A working `delete_record` default.**

`delete_record` (`resource/mod.rs`) defaults to an error, so the showcase defines
`delete_through_query!` (`examples/showcase/src/app.rs`) and invokes it for all four resources. Its
body is the obvious default: a query-scoped delete by primary key. Policy already gates
reachability: `can_delete_any` defaults to `false`, and the handler answers 403 before reading the
body.

**Change.** Default `delete_record` to the query-scoped delete. `bulk_delete_records` already
defaults to a loop over `delete_record`, so it works unchanged.

**Removes.** The showcase macro and its four invocations, and a resource that builds and then fails
on every delete.

**S13 — `paginate(NonZeroUsize)`.**

`Table::paginate(per_page: usize)` (`table/mod.rs`) documents zero as a programmer error, then
guards it in `Table::missing_essentials`, the load path, the render path, and `panel/build.rs`.

**Change.** Take `NonZeroUsize`. Delete the four guards and their tests.

**Removal list.** Each item has no caller in `crates/`, `examples/`, `benchmarks/`, or `xtask/`.

| Item | Evidence | Action |
| --- | --- | --- |
| `Posted::into_form` | `form.rs`; `Posted` already `Deref`s to `F` | delete |
| `Table::render_delete_dialog` | `render/dialog.rs`; only the `_normalized` sibling is called | delete |
| `Table::render_live_filter_bar` | `render/filterbar.rs`; same | delete |
| `Table::render_live_search_bar` | `render/toolbar.rs`; same, and its doc names a showcase caller that does not exist | delete |
| `Table::render_live_with_state` | `render/core.rs`; same | delete |
| `Table::render_skeleton` | `render/skeleton.rs`; called only by its own tests | `pub(crate)` |
| `Table::actions_for` | `table/mod.rs`; one caller, `render/core.rs` | `pub(crate)` |
| `GroupDef` re-export | `resource/mod.rs`; private fields, built only inside `group_by` | drop from the re-export |
| `RowActions` re-export | `lib.rs`; its only setter, `Table::row_actions`, is `pub(crate)` | drop from the re-export |
| `live_search(bool)` | every call passes `true`; the default is `false` | `live_search()` |
| `search(bool)` / `filter_bar(bool)` | only `search(false).filter_bar(false)` is called | `hide_search()` / `hide_filter_bar()` |
| Unreferenced `tablo-ui` re-exports | `tablo-ui/src/lib.rs`; no reference outside `primitives/` | delete |
| CI bench `cargo check` | `ci.yml` runs clippy `--all-targets` on the same manifest next | drop the step |

**Doc claims.** `docs/dev/architecture.md` says `tablo-ui` owns ten browser scripts; there are
eleven. Its write-order list puts the `can_*` checks before the parse, but delete and bulk delete
check `can_view`/`can_delete` inside the transaction after the scoped re-load.

### Batch 3 — Mutation

**S15 — One commit tail.**

`commit_write` (`panel/forms/submit.rs`) is the create and edit tail: commit, `run_after_commit`,
flash, redirect. Delete and bulk delete (`panel/actions/delete.rs`, `panel/actions/bulk.rs`)
re-implement it, and each clones the record set so the after-commit hook still has a snapshot once
`delete_record` has consumed it.

**Change.** Move `commit_write` to a shared module, generic over the written value, and call it from
all four handlers. `delete_record` takes `&Self::Model` and `bulk_delete_records` takes
`&[Self::Model]`, so the handlers pass the snapshot and then move it into `Committed`.

**Removes.** Three tails to one, and the pre-delete clones.

**S9 — A check hook replaces the write delegation pair.**

`create_record` and `update_record` (`resource/mod.rs`) default to the free functions
`write_create`/`write_update` (`form.rs`). A resource that needs a check inside the transaction
overrides the whole method and ends by calling the free function, because an override cannot call
the default. The showcase does this for posts and comments.

**Change.** `fn check_create(cx, &Self::Form, ex) -> Result<()>` and
`fn check_update(cx, &Self::Model, &Posted<Self::Form>, ex) -> Result<()>`, defaulting to `Ok(())`,
called inside the transaction after the parse and the unique probe. Delete `create_record`,
`update_record`, `write_create`, and `write_update` from the public surface.

**Removes.** Four public names to two, and the override-then-delegate pattern in the guide.

### Batch 4 — Declaration

**S4 — Compile the field list once.**

`Schema` (`schema/mod.rs`) re-walks its tree behind `leaves`, `any_leaf`, `text_inputs`,
`select_inputs`, `file_uploads`, `has_file_upload`, `field_names`, `unknown_keys`, and
`normalize_values`. Each collector clones every field it collects, and a create POST reads several.

**Change.** `Schema { nodes, fields: Vec<Field> }` with `Node::Field(usize)`, built in the walk
`assert_unique_field_names` already runs. Keep one `fields()` iterator; delete the other accessors
and the per-kind matches in `schema/tree.rs`.

**Removes.** Nine accessors to one, and repeated walks over cloned maps.

**S3 — One field type.**

`TextInput`, `Textarea`, `Select`, and `FileUpload` (`schema/fields/`) each repeat `validate`, the
read-only head, and the `required`/`optional`/`label` builders, across nine constructors. Only
`TextInput` and `Textarea` have a `_context` constructor, so an embedded `Select` or `FileUpload`
cannot bind. The `EmbeddedForm` derive already treats a textarea as a modifier
(`#[form(textarea, rows = N)]`).

**Change.** `Field { name, label, required, rules, control }` with
`Control::{Text { input_type, rows }, Choice { .. }, File }`. Constructors: `Field::text(path)`,
`Field::choice(path)`, `Field::file(path)`, each taking a resolved lens so the `_context` split
disappears; `.multiline(rows)` replaces `Textarea`.

**Removes.** Four field types to one, nine constructors to three, and the `_context` asymmetry.

**S6 — One render entry, one validate entry.**

`Schema::render` has no production caller; it is `render_with` with two empty maps.
`render_readonly` (`panel/detail.rs`) and `render_with` (`panel/forms/render.rs`) differ only in the
`Mode` the walk already carries. `Select::validate_async` (`schema/fields/select.rs`) has only test
callers; `Schema::validate_async` calls `validate_exists` directly.

**Change.** `schema.render(cx, Source::form(values, errors))` and `Source::view(values)`. Delete
`render_readonly`, `render_with`, and `Select::validate_async`.

**Removes.** Three render entries to one and two validation entries to one.

**S5 — Delete `Tabs`.**

`Tabs` (`schema/layouts.rs`) renders a stacked `div`, and its doc says so. Its only callers are its
own tests; `CONTEXT.md` and the guide name it.

**Change.** Delete it, its `Node` variant, its root re-export, and the doc mentions.

**S7 — An embedded value is a schema node.**

Nine root re-exports hand the derives key names and variant lists at request time: `value_keys`,
`read_embedded`, `write_embedded`, `leaf_key`, `parse_leaf`, `enum_spec`, `discriminant_select`,
`EnumSpec`, `TypedValue`. Their only callers are the two derives and tests. `EnumSpec` plus
`discriminant_select` (`schema/embedded.rs`) and `resolve_embedded_value` (`schema/lenses.rs`)
rebuild per request what a node could hold.

**Change.** `Node::Embedded` holding the resolved path, its child fields, and for an enum the
discriminant field and one group per variant. Keys come from the children and variant hiding is a
node property. Only `EmbeddedForm::{read_form, write_form}` stays public.

**Removes.** Nine public items to one trait, and the per-request rebuild.

**S1 — The form renders the detail page.**

`Resource::view`, `view_values`, and `viewed` (`resource/mod.rs`) sit beside `Resource::form`.
`view_values` has no override in the tree; `panel/detail.rs` renders `view()` from `view_values`
extended by `R::Form::hydrate`, and nothing ties the map's keys to the schema's, so a renamed field
renders blank. The showcase repeats its selects in `view` and `form` for users and posts.

**Change.** Delete `view`, `view_values`, and `viewed`. The detail page renders `form(cx)` read-only
from `R::Form::hydrate`; a `.detail_hidden()` field modifier omits a key, such as a foreign key the
page shows through `view_relations`. A resource has a detail page when its form is not empty, and
`can_view` gates it; a `NoForm` resource has none. Amend ADR-0016 and ADR-0022.

**Removes.** Three trait methods, the showcase's second schemas, and the view/values drift.

### Batch 5 — List

**S12 — Split the loader out of `Table`.**

`Table<M>` (`table/mod.rs`) holds its declaration beside `apply_declaration`, `load`, and
`load_with_probe`. `pub Table::load`'s only non-test caller is the benchmark, which pairs it with
`scoped_query` to mirror `panel::load_table_page`, a `pub(crate)` function it cannot call. Every
production path paginates: the panel refuses a table with no page size (`panel/list.rs`), so the
unpaginated branch serves only `pub Table::load`.

**Change.** `Table` keeps the declaration and pure planning, with no `Cx` and no `Db`. The loader
moves to the panel and is published as `ListPage::<R>::load(cx)`, which the list, the shard, and the
benchmark call. Delete `pub Table::load` and the unpaginated branch.

**Removes.** One public method, the benchmark's mirror, and the `Db` a `Table` test needs.

**S14 — One pager for the list and the export.**

`ExportChunker` (`panel/actions/export.rs`) and the paginated branch of `load_with_probe` each build
`Paginate`, decode cursors, treat "no cursor" as the end, and restate Toasty's cursor semantics.

**Change.** One `Pager { query, cursor, done }` with `next(&mut db, take)` beside the S12 loader;
the export wraps it for its row cap.

**Removes.** One of the two encodings of Toasty's cursor semantics.

**S11 — One cursor type.**

`TableState` (`resource/state.rs`) carries `after` and `before` as separate fields, beside the
`after:`/`before:` wire. Toasty accepts one cursor, so the pair admits an unrepresentable state.
`NormalizedState` (`table/mod.rs`) then doubles every render seam into a public method and a
`_normalized` twin: `render/core.rs`, `skeleton.rs`, `dialog.rs`, `toolbar.rs`, `filterbar.rs`.

**Change.** `cursor: Option<Cursor>` with `Cursor::{After, Before}`. The normalizer returns
`TableState`; delete the `_normalized` methods.

**Removes.** The unrepresentable pair, and half the table render surface.

**S10 — The URL is the only list state.**

`TableSignals` (`resource/state.rs`) carries one signal per state field, and the shard
(`panel/search.rs`) takes one argument per field. One keystroke goes from the debounce through a
hidden transport, the signals, the shard arguments, `from_live_args`, and the normalizer, while
every control already renders the complete URL in its `href`. The state has three spellings: URL
parameter, signal field, and the `after:`/`before:` wire.

**Change.** `TableSignals` becomes one `Signal<String>` holding the list's query string, still keyed
by list path. A control writes its own `href` query; the shard takes `(path, query)` and parses it
with `TableState::from_query`, the GET path's parser. Delete `from_live_args`, the cursor-wire
helpers, `TableSearchArgs`, and the per-field signal plumbing. One string needs no struct-typed
signal, so this closes #337.

**Removes.** Two of the three state spellings, and the "GET and live agree" tests, since one parser
serves both.

**S2 — Name the two loader queries; delete the include declaration.**

`IncludeNeeds` (`resource/column.rs`) is non-empty at two call sites, the list (`panel/list.rs`) and
the export (`panel/actions/export.rs`), and both pass the union of every column's `needs`. Every
other loader passes `IncludeNeeds::default()`. A wrong name in `TextColumn::needs` is not a compile
error: it renders `"(unloaded)"` or panics in `Deferred::get`.

**Change.** Keep the narrowing, which is real, and name it: `fn query(cx)` for the list, detail, and
export, and `fn query_record(cx)` for options, probes, and write re-loads. Delete `IncludeNeeds`,
`TextColumn::needs`, `include_names`, `Table::include_needs`, `query_with`, `export_query`, and
`scoped_query_with`.

**Removes.** Two trait methods, the include declaration, and its runtime failure mode.

**S27 — One URL parameter per filter.**

The `filters` parameter (`resource/state.rs`) nests `key:value,key2:value2` in one value, which
needs its own escaping, a size cap, the `filters=overflow` sentinel, and a mirror in
`tablo-ui/assets/filters.js`.

**Change.** `?f.status=published`, one parameter per active filter, parsed by the S10 parser. Old
filtered URLs stop filtering.

**Removes.** The nested grammar, its escaper, the overflow sentinel, and the script mirror.

### Batch 6 — View

**S17 — The skeleton derives its chrome from the loaded table.**

`render/core.rs` and `render/skeleton.rs` each spell the root and boundary `div`. The skeleton
always renders a toolbar pulse and a pager pulse; the loaded table gates the search bar on
`search_enabled()`, the filter bar on `filter_bar_enabled()`, and the pager on pagination. The guard
test compares only the `<table>` opening tag.

**Change.** One `table_root`/`table_boundary` used by both, and each pulse gated on the loaded
table's predicate.

**Removes.** The layout jump on swap and one class of unchecked drift.

**S18 — One live-or-plain link helper.**

`render/core.rs` (twice), `render/filterbar.rs`, and `render/pager.rs` (twice) each match on the
signals to emit either `href` plus a click handler or `href` alone.

**Change.** One `live_link(cx, url, signals) -> Attributes`. After S10 the handler writes the URL's
query, so the helper takes no per-control write.

**S19 — Split `render_inner`.**

`render_inner` (`render/core.rs`) inlines the bulk cell, the cell loop, the row action menu, the
empty-versus-rows branch, the revision signal, and the chrome wrapper, against the
`too_many_lines` budget.

**Change.** Extract `render_row` and `render_actions`, and move the head, rows, widths, and empty
state into `render/` modules, the shape `toolbar`, `filterbar`, `pager`, `dialog`, and `skeleton`
use.

**S28 — Shared markup pieces.**

- `render/dialog.rs` and `render/toolbar.rs` each spell the confirm dialog: Cancel, `confirm=1`, the
  CSRF field, a destructive submit, and a DOM-id formula. Extract one confirm dialog.
- `schema/fields/select.rs` renders `<option>` with `view!`, `panel/actions/options.rs` builds the
  same markup as a string through `escape_option`, and `tablo-ui/assets/selects.js` builds it a
  third time. The options endpoint renders through the field's renderer; delete `escape_option`.
- The render layer repeats the bar class, the quiet link class, and `button_variants` as literals
  beside the `class!` constants. Use the constants.

### Batch 7 — Infrastructure

**S20 — Delete the `auth` cargo feature.**

`crates/tablo-core/Cargo.toml` gates `argon2`, `password-hash`, and `topcoat/session` behind the
default `auth` feature, and `auth_off.rs` stands in when it is off. No workspace member builds it
off except `tablo-test`, which mirrors an empty `auth = []` feature to avoid Cargo feature
unification (`crates/tablo-test/Cargo.toml`).

**Change.** Delete the feature, `auth_off.rs`, and every `cfg(feature = "auth")`. `Auth::disabled()`
stays the opt-out (ADR-0013).

**Removes.** A parallel public API, the `tablo-test` workaround, and optional dependencies every
build resolves.

**S24 — One error vocabulary.**

A form re-render carries `FieldError`/`FieldErrorKind` (`form.rs`), `FieldErrors<F>`, and
`HashMap<String, Vec<String>>` (`schema/mod.rs`), merged by hand in `panel/forms/submit.rs`.
`hook_failure` (`db.rs`) and `infrastructure_failure` (`auth.rs`) are the same is-Toasty-then-map
check. Dozens of sites build domain errors as `std::io::Error::other(..)`, which `cursor.rs` and
`table/mod.rs` then classify by downcast.

**Change.** One keyed `FieldErrors` carrying `(key, message, required)`. One mapping for both
infrastructure failures. A crate `TabloError` enum (`Cursor`, `CursorRejected`, `Declaration`,
`Stub`, `Infrastructure`) with one conversion into `topcoat::Error`.

**Removes.** Two error-map vocabularies, one duplicate mapping, and classification by downcast.

**S25 — Delete the `OptionSource` shim; share the column machinery.**

`OptionSource` (`schema/relationship.rs`) and its blanket impl (`resource/mod.rs`) forward
`can_view_any`, `can_view`, `requires_tenant`, and `slug`, and derive `search_expr` and `order_by`
from `R::table()`. It exists so `schema` does not depend on `resource`, for one non-`Resource`
implementor (`examples/showcase/src/media.rs`). Separately, `resource/relation.rs` redeclares
`WIDE_COLUMN_MIN_REM` from `render/core.rs`, re-derives the width arithmetic in `relation_widths`,
and duplicates `into_columns_tuples!` as `into_relation_columns_tuples!`.

**Change.** Make the media library a `Resource` and let `Select::relationship` take `R: Resource`;
delete the shim. Share the width arithmetic and the tuple macro.

**Removes.** One trait, its blanket impl, and the second column implementation.

**S26 — One field classifier for both derives.**

`tablo-macros/src/embedded.rs` hardcodes the leaf type names, duplicating the `TypedValue` impls in
`schema/validation.rs` with no test linking the two. `record_form.rs` never classifies a field, so a
`Vec<String>` form field reaches rustc as E0277 on generated tokens with no field span. Each
record-form field emits and resolves its path and key three times.

**Change.** One `tablo-macros/src/fields.rs` with `enum FieldKind { Scalar, Embedded, Other }` and
shared attribute parsing. Both derives refuse `Other` with a spanned error naming the fix. Bind each
field's key and path once.

**Removes.** One of two field walkers, and an unspanned E0277.

### Batch 8 — Guide

**G1 — A first-panel chapter.**

The guide never shows `Panel::build`'s required `app_context` in code, and
`docs/guide/src/testing-and-benchmarks.md` points at `CxTestBuilder`, which `tablo-core` does not
re-export, without naming `crates/tablo-test`.

**Change.** One chapter with a complete `main`, and `tablo_test::TestClient` in the testing chapter.

**G2 — A document constructor for public pages.**

`docs/guide/src/panel-and-routing.md` shows a public layout that rebuilds the panel's head by hand,
then explains the `try_app_context::<AssetConfig>(cx).is_some()` guard it needs without assets.

**Change.** A public `Panel` layout constructor that reuses the shell's head and its asset fallback.

## 4. Do not change

Each of these looks like a simplification until it is read.

- **The CSRF token.** Topcoat's origin layer rejects a cross-site state-changing request, and the
  session cookie is `SameSite=Lax`, so the token is redundant on the shipped router. It is still the
  only defense for an app that mounts Tablo behind its own router or strips `Origin` and
  `Sec-Fetch-Site`, and it costs one hidden field.
- **Carrying an upload across a re-render.** `Uploader::holds` and the `keep_` control keep a chosen
  file when a validation error re-renders the form; dropping them makes the user pick it again.
- **The security boundaries.** `sanitize_filename`, `is_windows_reserved_name`, and the RFC 5987
  decoder (`panel/forms/decode.rs`) cover a surface no crate in the tree covers; the CSRF compare is
  constant-time; `infrastructure_failure` keeps driver text out of responses.
- **The tenancy code.** Small, and `tenant_field_index` serves both the filter builder and the build
  checks.
- **`db.rs`, `commit.rs`, `notification.rs`.** `db(cx)` is called from every loader and handler,
  `Committed` from every mutation, and the flash rides Topcoat's `cookie_store`.
- **`crates/tablo-test`.** Both integration suites call every export.
- **The root re-exports an app writes in a bound or signature** (`FieldLens`, `IntoSchema`,
  `NavTarget`, `TablePage`, `RowKey`, `FieldErrorKind`).
- **App-facing knobs nothing in the tree turns**: `Panel::frame_ancestors`, `Panel::dark_mode`,
  `ColumnWidth::{Rem, Percent}`, `auth::revoke_sessions_for_user`. Each is a documented feature.
- **The vendored primitives and their xtask sync.** `verify-topcoat-ui` makes drift fail the build.
- **The client scripts.** Topcoat hydrates once from its own render units, so swapped markup would
  leave every binding and shard marker dead; htmx does not replace them.
- **`tablo-ui` as a crate.** It is the sync unit for the vendored set.
