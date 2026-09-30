# Simplification spec

Where the workspace is larger or more intricate than its feature set requires, and the change that
removes the excess. Each item states the current code, the change, and what the change removes.

Baseline: `master` at `c3c967d3`. Every item cites the file and the symbol it concerns, not a line.
Tablo is experimental: an item may break the public API, and none keeps a compatibility shim.

## 1. Target state

**Declaration.** One `Resource` trait declares a resource: its model, its record form, its list
table, its policy, and its record functions. The detail page renders its own schema, whose keys are
checked against the values it renders. One field type with a control discriminant covers text,
multi-line text, choice, and file. Field metadata is resolved once when the schema is built.

**List.** The URL query string is the only list state; the live shard and the GET path parse it with
one function. One cursor type. One public loader, `TablePage::load`, used by the list, the shard,
a page-owned table, and the benchmark; the export shares its declaration, includes, and cursor
probe. `Table` is a declaration plus pure planning, with no `Cx` and
no `Db`. A column declares the relations it reads as typed paths. Every table paginates, and a page
size cannot be zero.

**Mutation.** One write pipeline: auth, body, CSRF, transaction, scoped re-load, policy, record fn,
commit, `after_commit`. Delete and bulk delete share the create and update tail.

**View.** One render entry per widget, taking the state it renders. The skeleton and the loaded
table derive their chrome from one source. One `<option>` renderer.

**Infrastructure.** One authentication configuration (`Auth::disabled()` is the opt-out). One error
vocabulary. No `auth` cargo feature.

## 2. Order

Six PRs, grouped by the files that change together so no code is rewritten twice. Each ends with the
gate set for what it touched.

| PR | Items | Why together |
| --- | --- | --- |
| 1. Tests out of source files | T1 | Mechanical and touches every file; alone and first, checked by an unchanged test count. |
| 2. Fixes and dead code | S22, S8, S16, S13, S15, S20, removal list, doc claims | Small, independent, low risk; one review pass. |
| 3. Field types and derives | S4, S3, S6, S5, S7, S1, S26 | S26 generates the field API S3 and S7 define; S4's field list underlies the rest. |
| 4. Loader | S12, S14, S2, S25 | One file set: `table/mod.rs`, `column.rs`, `panel/list.rs`, `export.rs`; S2's includes plug into S12's loader. |
| 5. List state and table rendering | S11, S10, S27, S18, S17, S19, S28 | The URL state, its links, and the render methods are one surface; S18 follows S10. |
| 6. Errors and guide | S24, G1, G2 | S24 is cross-cutting, so it converts only the code the refactors left; the guide describes the final API. The spec is deleted here. |

PR 1, then PR 2. PR 3 runs in parallel with PRs 4 and 5, which share only `resource/mod.rs`; PR 5
follows PR 4, and PR 6 lands last.

The upstream gaps #397, #398, and #399 block nothing. Each lands as its own follow-up when upstream
ships: #397 deletes the cursor probe in the S12 loader, #398 deletes `cursor.rs`, and #399 deletes
the URL builder S10 and S27 keep.

## 3. Items

### PR 1 — Tests out of source files

**T1 — Move inline test modules beside their subjects.**

About half of `crates/tablo-core/src` is `#[cfg(test)] mod tests` blocks. `panel/list.rs`,
`panel/actions/export.rs`, and `panel/build.rs` are between two thirds and four fifths tests. The
xtask asset-hook scanner (`production_sources`) cuts a bodiless `#[cfg(test)]` item such as
`mod test_support;` at the next item's closing brace, so it drops production code from the check.

**Change.** Every inline test module in every crate becomes `#[cfg(test)] mod tests;` with its body
in a sibling `tests.rs`: one rule, with no threshold to re-check. No test changes. The scanner cuts
a bodiless item at its `;` and skips `tests.rs` files.

**Removes.** Nothing. A source file reads as its production code.

### PR 2 — Fixes and dead code

**S22 — Method-scope the login gate bypass.**

The auth gate (`auth.rs`) passes any method at `{prefix}/login` through unauthenticated, while the
logout bypass below it is scoped to POST. An app route at `{prefix}/login` under PUT or DELETE runs
unauthenticated.

**Change.** Pass through only GET and POST at the login path.

**S8 — A singular label.**

`navigation_label()` (`resource/mod.rs`) is plural and titles singular actions:
`panel/list.rs`, `panel/forms/render.rs`, and `panel/forms/submit.rs` render `Create {label}`, and
`panel/forms/submit.rs` and `panel/forms/common.rs` render `Edit {label}`, so the showcase serves
"Create Blog Posts".

**Change.** Add `fn label() -> String`, singular, defaulted from the model type name, as Filament's
model label is. Derive `navigation_label()` as its plural through `pluralize`
(`resource/naming.rs`), and use `label()` in the action titles. `slug()` keeps deriving from the
resource type, so two resources over one model keep distinct routes.

**Removes.** The wrong titles.

**S16 — A working `delete_record` default.**

`delete_record` (`resource/mod.rs`) defaults to an error, so the showcase defines
`delete_through_query!` (`examples/showcase/src/app.rs`) and invokes it for all four resources. Its
body is the obvious default: a query-scoped delete by primary key. Policy already gates
reachability: `can_delete_any` defaults to `false`, and the handler answers 403 before reading the
body.

**Change.** Default `delete_record` to a delete through `scoped_query`, filtered to the table's
record key, which is the model's primary key. `bulk_delete_records` already defaults to a loop over
`delete_record`, so it works unchanged and a soft-delete override covers both.

**Removes.** The showcase macro and its four invocations, and a resource that builds and then fails
on every delete.

**S13 — Every table paginates.**

`Table::paginate(per_page: usize)` (`table/mod.rs`) documents zero as a programmer error, then
guards it in `Table::missing_essentials`, the load path, the render path, and `panel/build.rs`. A
table with no page size loads unbounded, and `load_table_page` (`panel/list.rs`) refuses it for a
resource list.

**Change.** A table paginates at 25 rows unless it declares otherwise, as Filament's tables do. The
page size is a `NonZeroUsize`; `paginate(usize)` panics on zero like `Table::new`'s other
misdeclarations, and `Panel::build` calls `Resource::table`, so it surfaces at boot. Delete the zero
guards, the unbounded branch, and the resource-list refusal, with their tests. Every table then
falls back to PK order, so `OrderMode` goes.

**Removes.** Four zero guards, one refusal, the unbounded load, and `OrderMode`.

**S15 — One commit tail.**

`commit_write` (`panel/forms/submit.rs`) is the create and edit tail: commit, `run_after_commit`,
flash, redirect. Delete and bulk delete (`panel/actions/delete.rs`, `panel/actions/bulk.rs`)
re-implement it, and each clones the record set so the after-commit hook still has a snapshot once
`delete_record` has consumed it.

**Change.** Move `commit_write` to a shared module, generic over the written value, and call it from
all four handlers. `delete_record` takes `&Self::Model` and `bulk_delete_records` takes
`&[Self::Model]`, so the handlers pass the snapshot and then move it into `Committed`.

**Removes.** Three tails to one, and the pre-delete clones.

**S20 — Delete the `auth` cargo feature.**

`crates/tablo-core/Cargo.toml` gates `argon2`, `password-hash`, and `topcoat/session` behind the
default `auth` feature, and `auth_off.rs` stands in when it is off. No workspace member builds it
off except `tablo-test`, which mirrors an empty `auth = []` feature to avoid Cargo feature
unification (`crates/tablo-test/Cargo.toml`).

**Change.** Delete the feature, `auth_off.rs`, and every `cfg(feature = "auth")`. `Auth::disabled()`
stays the opt-out (ADR-0013).

**Removes.** A parallel public API, the `tablo-test` workaround, and optional dependencies every
build resolves.

**Removal list.** Each item has no caller in `crates/`, `examples/`, `benchmarks/`, or `xtask/`.

| Item | Evidence | Action |
| --- | --- | --- |
| `Posted::into_form` | `form.rs`; `Posted` already `Deref`s to `F` | delete |
| `Table::render_delete_dialog` | `render/dialog.rs`; only the `_normalized` sibling is called | delete |
| `Table::render_live_filter_bar` | `render/filterbar.rs`; same | delete |
| `Table::render_live_search_bar` | `render/toolbar.rs`; same, and its doc names a showcase caller that does not exist | delete |
| `Table::render_live_with_state` | `render/core.rs`; same | delete |
| `Table::render_skeleton` | `render/skeleton.rs`; called only by its own tests | delete; tests call the `_normalized` twin |
| `Table::actions_for` | `table/mod.rs`; one caller, `render/core.rs` | `pub(crate)` |
| `GroupDef` re-export | `resource/mod.rs`; private fields, built only inside `group_by` | drop from the re-export |
| `RowActions` re-export | `lib.rs`; its only setter, `Table::row_actions`, is `pub(crate)` | drop from the re-export |
| `live_search(bool)` | every call passes `true`; the default is `false` | `live_search()` |
| `search(bool)` / `filter_bar(bool)` | only `search(false).filter_bar(false)` is called | `hide_search()` / `hide_filter_bar()` |
| CI bench `cargo check` | `ci.yml` runs clippy `--all-targets` on the same manifest next | drop the step |

**Doc claims.** `docs/dev/architecture.md` says `tablo-ui` owns ten browser scripts; there are
eleven. Its write-order list puts the `can_*` checks before the parse, but delete and bulk delete
check `can_view`/`can_delete` inside the transaction after the scoped re-load.

### PR 3 — Field types and derives

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

**Change.** `Field { name, label, required, rules, control }` with `Control::{Text { input_type,
rows }, Choice { .. }, File }`. Constructors: `Field::text(path)` for any `FormScalar` path
(`String`, a `TypedValue`, or an `Option` of one), `Field::choice(path)`, and `Field::file(path)`,
each taking a resolved lens so the `_context` split disappears. `.multiline(rows)` replaces
`Textarea`; `.email()`, `.unique()`, and `.placeholder()` stay text-control modifiers.

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

**S1 — Check the detail page's keys.**

`panel/detail.rs` renders `R::view(cx)` from `view_values` extended by `R::Form::hydrate`. Nothing
ties the map's keys to the schema's, so a field whose key neither supplies renders blank, silently.

**Change.** A view field whose key is absent from the map renders `(missing)` and trips a
`debug_assert!`, the contract the list columns keep for an unloaded relation (ADR-0011). `view`,
`view_values`, and `viewed` stay: like Filament's infolist, the detail page has its own layout,
shows keys the form does not, and exists for a list-only resource.

**Removes.** The silent blank.

**S26 — One field classifier for both derives.**

`tablo-macros/src/embedded.rs` classifies a leaf by type name against `PRIMITIVES`, which duplicates
the `TypedValue` impls with no test linking the two and refuses an app type that implements
`TypedValue`, which the guide invites. `record_form.rs` marks an embedded field with
`#[record_form(embed)]` and binds every other field as a scalar unchecked, so a `Vec<String>` field
reaches rustc as E0277 on generated tokens with no field span. Each record-form field emits and
resolves its path and key three times.

**Change.** One `tablo-macros/src/fields.rs` shared by both derives: a field is embedded when marked
`#[form(embed)]`, and a scalar otherwise. A scalar emits a `FormScalar` bound assertion spanned on
the field, so the error names the field and the fix. Delete `PRIMITIVES`. Bind each field's key and
path once.

**Removes.** The type-name table, one of two field walkers, and an unspanned E0277.

**As implemented (PR 3).** Where the implementation departs from the items above:

- S4: `normalize_values` and `unknown_keys` stay as `Schema` operations, each one pass over the
  compiled field list; the other accessors are gone. `controls()` reports only the `Repeater` skip:
  a variant group holds only an embedded value's fields, whose record-form field always answers
  blank, so `SkippedBy::VariantGroup` could not fire.
- S3: a constructor takes `impl Into<ResolvedLens<M, T>>`. A column's lens converts through `From`;
  an embedded path resolves with `ResolvedLens::new(cx, path)`, for every kind. `TypedValue` trades
  `accepts` for `parse_input` and gains `INPUT_TYPE`, which replaces the type-name timestamp check;
  it stays public as `tablo_core::schema::TypedValue`. A modifier on another control panics, naming
  the field. The unique probe compares through the field's own lens.
- S7: `Group::variant` is removed. The derive builds the node through `EmbeddedBuilder`, and the
  builder and parse helpers stay hidden (macro support) behind `__macro`, not removed. The node is
  still built per request — once per codec call — so the "per-request rebuild" in *Removes* above
  becomes one build per call rather than none. A view renders only the stored variant's group and the
  shared columns it declares.
- S26: both derives read one attribute, `#[form(..)]`: `#[form(model = ..)]`, `#[form(embed)]`,
  `#[form(blank = ..)]`, `#[form(label = ..)]`, and `#[form(multiline = N)]`. A derived leaf binds
  through a hidden `Field::embedded_leaf` whose only bound is `FormScalar`, so a bad leaf type fails
  once, at the field. A record-form field's path and key tokens are built once in the macro; each
  generated fn still resolves the key at run time.
- `unique()` implies presence only on a non-nullable column: an `Option` column stores NULL, which a
  unique index admits many times.

### PR 4 — Loader

**S12 — Split the loader out of `Table`.**

`Table<M>` (`table/mod.rs`) holds its declaration beside `apply_declaration`, `load`, and
`load_with_probe`. The panel calls the `pub(crate)` `load_with_probe` through `load_table_page`
(`panel/list.rs`); a page-owned table and the benchmark call `pub Table::load`, and the benchmark
pairs it with `scoped_query` to mirror `load_table_page`.

**Change.** `Table` keeps the declaration and pure planning, with no `Cx` and no `Db`. The loader
becomes one public constructor on the page it returns, `TablePage::load(cx, &table, query, &state)`,
in `resource/page.rs`: it applies the table's declaration and includes itself and reuses the bare
query for the cursor probes. The panel passes the scoped query, and a page-owned table and the
benchmark pass their own. Delete `Table::load` and `load_with_probe`.

**Removes.** The loader's second entry, its `probe_query` argument, and the benchmark's mirror.

**S14 — One pager for the list and the export.**

`ExportChunker` (`panel/actions/export.rs`) and the paginated branch of `load_with_probe` each build
`Paginate`, decode cursors, treat "no cursor" as the end, and restate Toasty's cursor semantics.

**Change.** The list loads one page in either direction; the export walks forward in chunks,
continues past an empty page that carries a cursor, and counts a row cap. What they share is the
one-row probe that asks whether a row exists past a cursor, so that becomes one
`row_exists_past(db, query, Past::After | Past::Before)` beside the S12 loader, used by the list's
cursor validation and the export's window check. A shared `Pager` would have to carry both walks'
rules, which is more code than it removes.

**Removes.** The duplicated probe. #397 deletes it outright.

**S2 — Typed column includes.**

A column declares the relations it reads by name (`TextColumn::needs`, `resource/column.rs`). The
list and the export gather them into `IncludeNeeds` and hand it to `Resource::query_with` or
`export_query` (`resource/mod.rs`), and the resource matches the names back to includes by hand: the
showcase's `base(needs)` helpers. Every other loader passes `IncludeNeeds::default()` through
`scoped_query_with`. A wrong name is not a compile error: the cell renders `"(unloaded)"` or panics
in `Deferred::get`.

**Change.** A column declares a typed path, `.include(Post::fields().author())`, and the loader
applies the list's includes itself, as Filament eager-loads a column's relationship. `query(cx)` is
row scoping only, used by every loader; `view_query(cx)` defaults to it and adds the detail page's
includes. Delete `IncludeNeeds`, `TextColumn::needs`, `include_names`, `Table::include_needs`,
`query_with`, `export_query`, and `scoped_query_with`.

**Removes.** Three trait methods to two, the name-matching helpers, and the wrong-name failure mode.

**S25 — Share the column machinery.**

`resource/relation.rs` redeclares `WIDE_COLUMN_MIN_REM` from `render/core.rs`, re-derives the width
arithmetic in `relation_widths`, and duplicates `into_columns_tuples!` as
`into_relation_columns_tuples!`.

**Change.** Share the width arithmetic and the tuple macro: `column.rs` holds the kind-default
budget, the cell style, the `MinWidth` terms, and one `column_tuples!` macro that both column-list
traits expand. Relation tables then follow the list's 60% budget for kind defaults.

**Removes.** The second column implementation.

### PR 5 — List state and table rendering

**S11 — One cursor type.**

`TableState` (`resource/state.rs`) carries `after` and `before` as separate fields, beside the
`after:`/`before:` wire. Toasty accepts one cursor, so the pair admits an unrepresentable state.
`NormalizedState` (`table/mod.rs`) then doubles every render seam into a public method and a
`_normalized` twin: `render/core.rs`, `skeleton.rs`, `dialog.rs`, `toolbar.rs`, `filterbar.rs`.

**Change.** `cursor: Option<Cursor>` with `Cursor::{After, Before}`; a URL naming both parses as
the first page, the recovery the cursor retry gives. The normalizer returns `TableState`; delete
`NormalizedState` and the `_normalized` methods.

**Removes.** The unrepresentable pair, and half the table render surface.

**S10 — The URL is the only list state.**

`TableSignals` (`resource/state.rs`) carries one signal per state field, and the shard
(`panel/search.rs`) takes one argument per field. One keystroke goes from the debounce through a
hidden transport, the signals, the shard arguments, `from_live_args`, and the normalizer, while
every control already renders the complete URL in its `href`. The state has three spellings: URL
parameter, signal field, and the `after:`/`before:` wire.

**Change.** `TableSignals` becomes two signals, still keyed by list path: `query`, the list's query
string, and `bulk`, the selection, which is not URL state and survives a rerun. A control writes its
own `href` query; the shard takes `(path, query)` and parses it with `TableState::from_query`, the
GET path's parser. Delete `from_live_args`, the cursor-wire helpers, `TableSearchArgs`, and the
per-field signal plumbing. Two strings need no struct-typed signal, so this closes #337.

**Removes.** Two of the three state spellings, and the "GET and live agree" tests, since one parser
serves both.

**S27 — One URL parameter per filter.**

The `filters` parameter (`resource/state.rs`) nests `key:value,key2:value2` in one value, which
needs its own escaping, a size cap, the `filters=overflow` sentinel, and a mirror in
`tablo-ui/assets/filters.js`.

**Change.** `?f.status=published`, one parameter per active filter, parsed by the S10 parser. The
filter controls are real `f.<name>` form fields, so a static filter bar is an ordinary GET form. One
bound stays, on count and length: past 32 filters, or past 256 bytes in a name or value, the
filter is dropped and flagged, so the export still refuses. An old `?filters=` URL no longer
filters; it is flagged the same way, so a saved link warns and its export refuses.

**Removes.** The nested grammar, its escaper, the overflow sentinel, the malformed channel, and the
script mirror.

**S18 — One live-or-plain link helper.**

`render/core.rs` (twice), `render/filterbar.rs`, and `render/pager.rs` (twice) each match on the
signals to emit either `href` plus a click handler or `href` alone.

**Change.** One `live_link(cx, url, signals) -> Attributes`. After S10 the handler writes the URL's
query, so the helper takes no per-control write.

**S17 — The skeleton derives its chrome from the loaded table.**

`render/core.rs` and `render/skeleton.rs` each spell the root and boundary `div`. The skeleton
always renders a toolbar pulse and a pager pulse; the loaded table gates the search bar on
`search_enabled()`, the filter bar on `filter_bar_enabled()`, and the pager on pagination. The guard
test compares only the `<table>` opening tag.

**Change.** One `table_root`/`table_boundary` used by both, and each pulse gated on the loaded
table's predicate.

**Removes.** The layout jump on swap and one class of unchecked drift.

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
  third time. The field, the filter bar and the options endpoint share one `option_view`; delete
  `escape_option`. `selects.js` keeps building the one option that preserves the current selection
  across a swap.
- The render layer repeats the bar class, the quiet link class, and `button_variants` as literals
  beside the `class!` constants. Use the constants.

### PR 6 — Errors and guide

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
- **The record fns.** `create_record` and `update_record` are the full override of a write, as
  Filament's `handleRecordCreation` is: an app sets server-side fields, writes related rows, or
  checks inside the transaction, then delegates to `write_create`/`write_update`. A check-only hook
  would cover the showcase and lose the rest.
- **`OptionSource`.** A relationship select draws from any source, not only a `Resource`: the
  showcase's cover picker reads the media library, a custom page with no list or form.
- **The `tablo-ui` re-exports.** An app builds its own pages from them (the showcase's live page
  uses `tablo_ui::page`), so an unused one is an unused building block, not dead code.
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
