# Simplification spec

Where the workspace is larger or more intricate than its feature set requires, and the change that
removes the excess. Each item states the current code, the change, and what the change removes.

Baseline: `master` with #382 and #383 applied. Every item cites the file and the symbol it concerns,
not a line, so a citation does not rot when the code moves. Items marked **[decision]** need a
product call rather than a patch.

Scope: every production module in `crates/tablo-core/src`, plus `crates/tablo-macros`,
`crates/tablo-ui`, `docs/guide/`, `xtask/`, the showcase, and the pinned Toasty and Topcoat
checkouts.

## 1. Target state

The end state the work items in §2 reach.

**Declaration.** One `Resource` trait declares everything about a resource: its model, its record
form, its list table, its policy, and its record functions. One schema per resource describes both
the form and the detail page. One field type with a control discriminant covers text, multi-line
text, choice, and file. Field metadata is resolved once when the schema is built, not per accessor.

**List.** The URL query string is the only list state; the live shard and the GET path parse it with
one function. One cursor type. One loader, used by the list, the shard, the export, a page-owned
table, and the benchmark. `Table` is a declaration plus pure planning, with no `Cx` and no `Db`. A
page size cannot be zero by construction.

**Mutation.** One write pipeline: auth, body, CSRF, transaction, scoped re-load, policy, record fn,
commit, `after_commit`. Create and update customize it through a check hook rather than by
reimplementing the tail. Delete and bulk delete share the same tail as create and update.

**View.** One render entry per widget, taking the state it renders. The skeleton and the loaded
table derive their chrome from one source. One markup escaper.

**Infrastructure.** One authentication configuration (`Auth::disabled()`). One error vocabulary. One
option source. The `auth` cargo feature does not exist.

## 2. Work items

### 2.1 Declaration and schema

**S1 — One schema per resource.**

`Resource::view` and `view_values` (`resource/mod.rs`) sit beside `Resource::form`. `view_values`
has no override anywhere in the tree, and `panel/detail.rs` renders `view()` from a map the form
already hydrates:

```rust
let mut values = R::view_values(cx, record);
values.extend(<R::Form as RecordForm>::hydrate(cx, record));
```

Nothing ties the map's keys to the schema's, so a renamed field renders blank. The showcase repeats
its status and featured selects in both `view` and `form` (`examples/showcase/src/app.rs`).

**Change.** Delete `view`, `view_values`, and `viewed`. The detail page renders `form(cx)`
read-only through `R::Form::hydrate`. Add one modifier (`.detail_hidden()`) for the keys a detail
page omits. `viewed` becomes "the form is not empty", so the route, the View link, and the rendered
schema still cannot disagree — now across one schema instead of two.

**Removes.** 3 trait methods, one concept, the showcase repetition, and the unchecked view/values
drift.

**S2 — Delete `export_query` and the include declaration.**

`export_query` (`resource/mod.rs`) defaults to `query_with`, and its only production caller is
`panel/actions/export.rs`. `IncludeNeeds` is non-empty at exactly two call sites — the list
(`panel/list.rs`) and the export (`export.rs`) — and both pass the union of every rendered
column. Every other loader passes `IncludeNeeds::default()`. Because the set cannot be derived, a
wrong name in `TextColumn::needs` is not a compile error: it renders `"(unloaded)"`
(`resource/column.rs`) or panics in `Deferred::get`.

**Change.** Keep the narrowing, which is real: option loads must not drag a post's comments into a
comment form. Name it instead of parameterizing it — `fn query(cx)` for the list and detail, `fn
query_record(cx)` for options and probes — and delete `IncludeNeeds`, `TextColumn::needs`,
`include_names`, `Table::include_needs` (`table/mod.rs`), `query_with`, `export_query`, and
`scoped_query_with`.

**Removes.** 2 trait methods, 4 concepts, one query per list load, and the `"unloaded"` failure mode.

**S3 — One leaf type.**

`TextInput`, `Textarea`, `Select`, and `FileUpload` repeat `validate`, the read-only head, and the
`required`/`optional`/`label` builders. Nine constructors resolve a lens and spell a struct literal.
Only `TextInput` and `Textarea` have a `_context` twin (`schema/fields/`), so an embedded `Select`
or `FileUpload` cannot bind at all. The `EmbeddedForm` derive already treats a textarea as a
modifier (`#[form(textarea, rows = N)]`), so the type system and the macro disagree.

**Change.** `Leaf { name, label, required, rules, control }` with
`Control::{Text { input_type, rows }, Choice { .. }, File }`; one constructor shape per kind;
`.multiline(rows)` instead of a type. Delete `Textarea` and five of the nine constructors.

**Removes.** 4 field types to 1, 9 constructors to 3, and the `_context` asymmetry.

**S4 — Compile the leaf list once.**

`Schema` re-walks its own tree behind nine accessors (`schema/mod.rs`): `leaves`, `any_leaf`,
`text_inputs`, `select_inputs`, `file_uploads`, `has_file_upload`, `field_names`, `unknown_keys`,
and `normalize_values`. Each collector re-walks the tree and clones every field it collects, and a
create POST reads several of them.

**Change.** `enum Node { Field(usize), Section(..), Group(..), Grid(..) }` plus
`Schema { nodes, fields: Vec<Field> }`, built in the walk `assert_unique_field_names`
already runs. Keep one `fields()` iterator. Delete the other accessors and the four-arm
matches in `schema/tree.rs`.

**Removes.** 9 accessors to 1, and one build pass plus one read instead of repeated walks over
cloned maps.

**S5 — Delete `Tabs`; fold `Repeater` into `Section`.**

`Tabs` renders a plain stacked `div` and its own doc says so (`schema/layouts.rs`). A workspace
grep finds only its own tests and one guide line, so it is public API with no caller. `Repeater`
renders the same border-only panel as `Section`; the only differences are `required` and the
`SkippedBy` split it forces.

**Change.** Delete `Tabs` (the `Node` variant, the macro entry, the guide line). Make
`Section::required()` the titled required group so the repeater branch of `walk_absent_groups` folds
into the generic one.

**Removes.** 5 layout blocks to 3, one `Node` variant, and the `SkippedBy` enum.

**S6 — One render entry, one validate entry.**

`Schema::render` (`schema/mod.rs`) has no production caller; it is `render_with` with two empty
maps. `render_readonly` and `render_with` differ only in the `Mode` flag the walk
already carries, and their production callers are `panel/forms/render.rs` and
`panel/detail.rs`. Separately, `Select::validate_async` (`schema/fields/select.rs`) repeats
`validate` plus `validate_exists`, which `Schema::validate_async` then re-runs per select.

**Change.** `schema.render(cx, &Source::form(values, errors))` and `Source::view(values)`; one
`Field::validate_async`. Delete `render`, `render_readonly`, `render_with`,
`Select::validate_async`, `SkippedBy`, and the empty-map requiredness probe.

**Removes.** 4 render entries to 1, 2 validation entries to 1, and the required message worded once.

**S7 — An embedded value is a schema node.**

Nine root re-exports exist to hand the derives column names and variant lists at request time:
`value_keys`, `read_embedded`, `write_embedded`, `leaf_key`, `parse_leaf`, `enum_spec`,
`discriminant_select`, `EnumSpec`, `TypedValue`. Their only callers are the two derives and tests.
`EnumSpec` plus `discriminant_select` (`schema/embedded.rs`) and `resolve_embedded_value` with
its four collectors (`schema/lenses.rs`) rebuild per request what a node could hold once.

**Change.** `Node::Embedded(EmbeddedGroup)` holding the resolved path, its child fields, and — for
an enum — the discriminant `Select` and per-variant `Group`s. Keys come from the node's children,
the discriminant is a child, and variant hiding is a node property. Only
`EmbeddedForm::{read_form, write_form}` survives.

**Removes.** 9 public items to one trait, and the per-request rebuild of what the node can hold.

**S8 — One name per resource.**

`navigation_label()` (`resource/mod.rs`) is plural ("Blog Posts") and is used where a singular
belongs: `panel/list.rs` and `panel/forms/render.rs` render `Create {label}`, and `panel/forms/submit.rs`
and `panel/forms/common.rs` render `Edit {label}`. The showcase therefore serves
"Create Blog Posts" and "Edit Blog Posts". `slug()` derives from the resource type while the
label derives from the model type, so `StaffResource` over `User` is `/staff` labelled "Users".

**Change.** Add `fn label() -> String`, singular, defaulted from the model type name; derive
`navigation_label()` as its plural; use `label()` at the five title sites. A wrong plural then costs
one override, so `pluralize`'s irregular, f-exception, and uncountable tables
(`resource/naming.rs`) can be deleted.

**Removes.** 5 wrong strings, the naming tables, and the two-source naming split.

**S9 — A check hook replaces the write delegation pair.**

`create_record` (`resource/mod.rs`) and `update_record` default to
`write_create`/`write_update` (`form.rs`). A resource that needs a check inside the
transaction must override the whole method and end with the free function
(`docs/guide/src/forms.md`), because Rust cannot call a default from an override. The showcase
does this four times.

**Change.** `fn check_create(cx, &Self::Form, ex) -> Result<()>` and `check_update(cx,
&Posted<Self::Form>, ex)`, default `Ok(())`, called by the handler inside the transaction after the
parse and the unique probe. Remove `create_record`, `update_record`, `write_create`, and
`write_update` from the public surface.

**Removes.** 4 public names to 2, and one layer per create.

### 2.2 List path

**S10 — The URL is the only list state.**

`TableSignals` (`resource/state.rs`) carries one signal per state field. One keystroke walks a
chain: the debounce, a hidden transport that writes `q` and the cursor, the signal, a shard
invocation that packs every handle, the shard's one wire argument per field, the registry,
`to_state`,
`normalize_state`, and `load_table_page`, which applies the declaration twice. Every control already
renders the complete URL, so each click re-derives by hand what its `href` states, in three
vocabularies: URL parameter, signal field, and the `after:`/`before:` wire.

**Change.** `TableSignals` becomes one `Signal<String>` holding the list's query string. A control
writes `state.query_string()`; the shard takes `(path, url)` and calls `TableState::from_query`.
Delete `from_live_args` (`state.rs`), the four cursor-wire helpers, `TableSearchArgs`, and the
per-field arms of `to_signals`/`to_state`. Upstream, #337 asks Topcoat for the struct-typed shard
signal this needs, and names the same retirement condition.

**Removes.** 4 concepts, and the "GET and live must agree" test because one parse path serves both.

**S11 — One cursor type; delete the `_normalized` twins.**

`TableState` carries `after` and `before` separately, the `after:<token>` wire, and the cursor token.
Toasty accepts exactly one cursor, so the pair is unrepresentable — the code says so
(`state.rs`) and keeps the pair anyway. `NormalizedState` (`table/mod.rs`) then doubles every
render seam: `render_with_state` and `render_normalized` (`render/core.rs`),
`render_live_with_state` and `render_live_normalized`, and the same pair on the
skeleton (`skeleton.rs`), the delete dialog (`dialog.rs`), the live search bar (`toolbar.rs`), and
the live filter bar (`filterbar.rs`). Each public half calls the private half after one clone of a
small map.

**Change.** `cursor: Option<Cursor>` with `After`/`Before`. Make the normalizer return `TableState`
and delete the `_normalized` methods.

**Removes.** 5 cursor representations to 2, and halves the table render surface.

**S12 — Split the table loader out of `Table`.**

`Table<M>` holds its declaration fields beside `apply_declaration`, `load` (`table/mod.rs`), and
`load_with_probe`. `pub Table::load` has no production caller: every caller in the workspace is a
test, and `benchmarks/tablo/src/main.rs` re-implements the pipeline because the real loader is
`pub(crate)`. Once `pub Table::load` goes, the unpaginated branch of `load_with_probe` becomes
unreachable: the panel refuses a table with no page size (`panel/list.rs`), so every remaining
caller paginates.

**Change.** `Table` keeps the declaration and pure planning, with no `Cx` and no `Db`. Loading moves
beside its only production caller and is published as one value, `ListPage::<R>::load(cx)`, so the
list, the shard, a page-owned table, and the benchmark all call the real loader. Delete
`pub Table::load` and the unreachable branch.

**Removes.** One public method, the benchmark's hand-mirrored copy of the loader, and the `Db`
dependency a `Table` test needs today.

**S13 — `paginate(NonZeroUsize)`.**

`Table::paginate(per_page: usize)` (`table/mod.rs`) documents zero as a programmer error, then
guards it four times: `Table::missing_essentials`, the load path, the render path, and
`panel/build.rs`.

**Change.** Take `NonZeroUsize`. Delete `missing_essentials` and the build branch.

**Removes.** Four guards, one build-error variant, and three tests.

**S14 — One pager for the list and the export.**

`ExportChunker` (`table/export.rs`) and the paginated branch of `load_with_probe` both build
`Paginate`, decode cursors, re-derive "no cursor means end", and restate Toasty's upper-bound
semantics (`export.rs` and `table/mod.rs`).

**Change.** One `Pager { query, after, done }` with `next(&mut db, take)` beside the loader; the
export wraps it for its cap and window accounting.

**Removes.** One of the two places that encode Toasty's cursor semantics.

### 2.3 Mutation path

**S15 — One commit tail.**

`commit_write` (`panel/forms/submit.rs`) is the shared tail: commit, `run_after_commit`, notify,
redirect. Create and edit use it. Delete and bulk delete re-implement the same body
(`panel/actions/delete.rs`, `panel/actions/bulk.rs`), including the flash and `see_other` sequence
that
`redirect_after_write` (`forms/common.rs`) already provides. `docs/dev/architecture.md`
documents one shape.

**Change.** Move `commit_write` to a shared module and widen it over `Result<T, _>` with
`FnOnce(T) -> Committed<R::Model>`; call it from all four handlers. In the same change, take
`&Model` and `&[Model]` in `delete_record` and `bulk_delete_records` (`resource/mod.rs`) so the
handlers stop cloning a pre-delete snapshot, and drop the `Clone` bound from `Resource::Model`.

**Removes.** Three tails to one, the duplicated handler bodies, and `Clone` from the
`Resource::Model` contract.

**S16 — A working `delete_record` default.**

`docs/guide/src/resources.md` documents that `delete_record` "defaults to an error naming the
type", and no guide chapter shows an implementation. The showcase therefore defines
`delete_through_query!` (`examples/showcase/src/app.rs`) and uses it four times. Its whole body is
the obvious default: `Self::query(cx).filter(Model::fields().id().eq(record.id)).delete()`.

Policy already gates reachability, so a resource that never considered delete cannot reach the stub:
`can_delete_any` defaults to `false` and the handler answers 403 before reading the body
(`panel/actions/delete.rs`). The stub only produces a resource that passes every build check and
fails on each click.

**Change.** Default `delete_record` to the query-scoped delete. Keep the error only for a model with
no PK-addressable route.

**Removes.** One app-side macro and its four invocations, and one 500-at-click failure mode.

### 2.4 View layer

**S17 — The skeleton derives its chrome from the loaded table.**

`render/core.rs` and `render/skeleton.rs` spell the same root `div`, and the same boundary `div`.
The skeleton's toolbar pulse and pager pulse render unconditionally, while the loaded table gates
the search bar on `search_enabled()`, the filter bar on `filter_bar_enabled()`, and the pager on
pagination. A table
with no searchable column shows a search pulse that disappears on swap, and the showcase Post table
renders one bar before the swap and two after it. The guard test compares only the `<table>` opening
tag, so it cannot see the drift.

**Change.** One `table_root`/`table_boundary` used by both, with each pulse gated on the predicate
the loaded table uses.

**Removes.** One class of unchecked drift and a visible layout jump on every list load.

**S18 — One live-or-plain link helper.**

`render/core.rs`, `render/filterbar.rs`, and `render/pager.rs` each repeat the same
`match signals { Some(s) => attributes! { href=(url) @click=$(..) }, None => attributes! { href=(url) } }`.

**Change.** One `live_or_plain(cx, url, signals, write) -> Attributes`.

**Removes.** The fallback and the cursor reset get one edit site instead of five.

**S19 — Split `render_inner`.**

`render_inner` (`render/core.rs`) inlines the bulk cell, the cell loop, the row action menu, the
empty-versus-rows branch, the revision signal, and the chrome wrapper, and is the function that
spends the `too_many_lines` budget on all six. The table's error state already lives elsewhere
(`panel/list.rs`), so the function is eight jobs with no boundary between them.

**Change.** Extract `render_row` and `render_actions`, and split `render/head.rs`, `rows.rs`,
`widths.rs`, and `empty.rs`, the shape `toolbar`, `filterbar`, `pager`, `dialog`, and `skeleton`
already use.

**Removes.** Nothing; the function comes back under the `too_many_lines` budget and four concerns
become files.

### 2.5 Infrastructure

**S20 — Delete the `auth` cargo feature.**

`crates/tablo-core/Cargo.toml` gates `argon2`, `password-hash`, and `topcoat/session` behind
`auth`. `auth_off.rs` is an `Auth` whose `is_disabled()` returns `true`, which is what `Auth::disabled()`
does. Runtime behavior is identical either way; what differs is the public API,
which lacks `Auth::password`, `Auth::custom`, `hash_password`, `AdminUser`, `AuthSession`, and
`Panel::login_hint` when the feature is off. No workspace member builds it off: the only
`default-features = false` consumer is `tablo-test`, which then mirrors an empty `auth = []` feature
and documents that it does so to avoid Cargo feature unification
(`crates/tablo-test/Cargo.toml`).

**Change.** Delete the feature and `auth_off.rs`. Keep `Auth::disabled()` as ADR-0013's explicit
opt-out. `argon2`, `password-hash`, and `session` become unconditional; every default build already
resolves them.

**Removes.** One parallel public API, every `feature = "auth"` site in the crate, one
Cargo-unification workaround, and the dependency optionality that buys nothing.

**S21 — Delete `csrf.rs`. [decision]**

`csrf.rs` reuses any valid existing cookie and mints only when absent: no rotation, no expiry of
its own, and no session binding. Two upstream layers already refuse the attack. Topcoat installs
`origin: OriginLayer::new(self.origin_policy)` unconditionally
(`topcoat-router/src/builder.rs`), and the default policy verifies every state-changing request,
rejecting a cross-site `Sec-Fetch-Site` or a mismatched `Origin`/`Host` with 403
(`topcoat-router/src/origin.rs`). The session cookie is forced `SameSite=Lax`
(`topcoat-session/src/token/store.rs`), so a cross-site POST carries no session. Tablo never
loosens either.

**Change.** Delete `csrf.rs`, the hidden field, and the verify calls; amend ADR-0013 and ADR-0021.

**Decision.** This trades a layer Tablo owns for two upstream layers it does not control. If a
downstream app installs its own router or strips `Origin` and `Sec-Fetch-Site`, the token is the only
remaining defense. Keep it only with documentation that says the protection is redundant on the
shipped router.

**S22 — Method-scope the login gate bypass.**

`auth.rs` returns unauthenticated for any method at `{prefix}/login`, while the logout bypass two
lines later is deliberately scoped to POST with a comment explaining why. An app route at
`{prefix}/login` under PUT or DELETE runs unauthenticated.

**Change.** `&& matches!(*method(cx), GET | POST)`.

**S23 — Delete the upload carry protocol.**

`Uploader::holds` (`upload.rs`), `restore_pending_uploads` (`forms/common.rs`), the
`clear_`/`keep_` prefixes (`common.rs`), the hidden control (`forms/render.rs`), and a
`carried` argument threaded through the form pipeline exist so a file survives a validation
re-render. The only implementation scans the upload directory on every re-render
(`examples/showcase/src/app.rs`).

**Change.** Drop `holds`, `keep_<field>`, and `carried`. A failed create re-renders with an empty
file input; an edit keeps the stored value, which is the existing untouched-field rule.

**Removes.** One directory scan per re-render, one hidden control, and one public trait method.

**S24 — One error vocabulary.**

Three vocabularies describe one re-render path: `FieldError`/`FieldErrorKind` (`form.rs`),
`FieldErrors<F>`, and `HashMap<String, Vec<String>>` (`schema/mod.rs`), merged by hand
in `submit.rs`. `db.rs` (`hook_failure`) and `auth.rs` (`infrastructure_failure`) are the same
is-toasty-then-map check. And dozens of sites build domain errors as
`std::io::Error::other(format!(..))` — cursor decode, a table declaration, a missing record fn —
which are then classified by downcast (`cursor.rs`, `table/mod.rs`).

**Change.** One keyed `FieldErrors` carrying `(key, message, required)`. One
`unavailable_with(error, || ..)` for both infrastructure mappings. A crate `TabloError` enum
(`Cursor`, `CursorRejected`, `Declaration`, `Stub`, `Infrastructure`) with one conversion into
`topcoat::Error`.

**Removes.** Two duplicate vocabularies, one duplicate mapping, and an error taxonomy that is
exhaustive instead of three predicates that must each remember to downcast.

**S25 — Delete the `OptionSource` shim; share the relation column machinery.**

`OptionSource` (`schema/relationship.rs`) plus its blanket impl (`resource/mod.rs`) forwards
`can_view_any`, `can_view`, `requires_tenant`, and `slug` unchanged and derives `search_expr` and
`order_by` from `R::table()`. It exists so `schema` does not depend on `resource`, for one
implementor that is not a `Resource` (`examples/showcase/src/media.rs`). Separately,
`resource/relation.rs` re-implements column machinery it should share: `WIDE_COLUMN_MIN_REM`
duplicates the constant in `render/core.rs`, `relation_widths` re-derives the same arithmetic, and
`into_relation_columns_tuples!` duplicates `into_columns_tuples!` in `resource/column.rs`.

**Change.** Make the one non-`Resource` implementor a `Resource`, or take the trait as a parameter,
and delete the shim. Share the width arithmetic and the tuple macro.

**Removes.** 2 concepts and the divergence between the two column implementations.

### 2.6 Macros

**S26 — One field classifier for both derives.**

`tablo-macros/src/embedded.rs` hardcodes a list of leaf type names, duplicating the `TypedValue`
impls in `schema/validation.rs`, with no test linking the two lists. `record_form.rs` treats
every non-`embed` field as a scalar and never classifies it, so a `Vec<String>` form field reaches
rustc as an E0277 on generated tokens with no field span. Each record-form field also emits and
resolves its path and key three times (`record_form.rs`), and a three-field embedded struct emits
two `chained(...)` paths per leaf.

**Change.** One `tablo-macros/src/fields.rs` with `enum FieldKind { Scalar, Embedded, Other }`
derived from one table, plus shared attribute parsing. Both derives emit a field-spanned refusal
naming both fixes. Bind each field's key and path once per body.

**Removes.** One of the two duplicated walkers, and an E0277 that names neither the field nor the
fix becomes a spanned message.

### 2.7 Tests

**T1 — Move inline test modules beside their subjects.**

About half of `crates/tablo-core/src` is `#[cfg(test)] mod tests` blocks, so a file's length says
more about its tests than its code. The largest production file is smaller than it looks, and
`panel/list.rs`, `panel/actions/export.rs`, and `panel/build.rs` are between half and four fifths
tests.

**Change.** `#[cfg(test)] mod tests;` in a sibling `tests.rs` per module, reusing the fixtures that
already exist (`panel/test_support.rs`, `test_support.rs`, `tests/common/mod.rs`). Add a gate on
production lines per file so the `too_many_lines` budget measures code rather than tests.

**Removes.** Nothing. It makes every source file about half as long to read and makes the
production-only outliers visible.

### 2.8 Guide

**G1 — A hello-world chapter.**

The guide never shows `Panel::build`'s required `app_context` (`panel/build.rs`) outside prose,
and `docs/guide/src/testing-and-benchmarks.md` points at `CxTestBuilder` — a `topcoat::context`
type `tablo-core` does not re-export — while never naming `crates/tablo-test`. Both gaps stop an
author before the first line of real code.

**Change.** One chapter with a complete `main`, and `tablo_test::TestClient` in the testing chapter.

**G2 — A document constructor for public pages.**

`docs/guide/src/panel-and-routing.md` shows a layout that duplicates the panel's head by hand,
and the prose after it explains the `try_app_context::<AssetConfig>(cx).is_some()` guard a markup
test needs.

**Change.** A public document or layout constructor on `Panel` that reuses the shell's head and its
asset fallback.

## 3. Removal list

Items with no caller in `crates/`, `examples/`, `benchmarks/`, or `xtask/`, each verified by grep.
The individual sizes are small; the value is the smaller public surface.

| Item | Evidence | Action |
| --- | --- | --- |
| `Posted::into_form` | `form.rs` — the definition is the only occurrence; `Posted` already `Deref`s to `F` | delete |
| `Table::render_delete_dialog` | `render/dialog.rs` — no caller; the `_normalized` sibling is live | delete |
| `Table::render_live_filter_bar` | `render/filterbar.rs` — no caller; the `_normalized` sibling is live | delete |
| `Table::render_live_search_bar` | `render/toolbar.rs` — no caller; the `_normalized` sibling is live, and its doc claims a showcase caller that does not exist | delete and fix the doc |
| `Table::render_live_with_state` | `render/core.rs` — no caller; the `_normalized` sibling is live | delete |
| `Table::render_skeleton` | `render/skeleton.rs` — called only by its own tests | make `pub(crate)` |
| `Table::actions_for` | `table/mod.rs` — one in-crate caller (`render/core.rs`) | make `pub(crate)` |
| `GroupDef` re-export | `table/mod.rs` — private fields, no `impl`, built only inside `group_by` | drop from the re-export |
| `RowActions` re-export | `lib.rs` — its only setter is `pub(crate)` | drop, or open the seam |
| `live_search(bool)` argument | every call site passes `true`; the default is already `false` | `live_search()` |
| `search(true)` / `filter_bar(true)` arms | never called; only `search(false).filter_bar(false)` is | `hide_search()` / `hide_filter_bar()` |
| Unreferenced `tablo-ui` re-exports | `tablo-ui/src/lib.rs` — no reference outside `primitives/` | delete; the sync guard still passes |
| CI bench `cargo check` | `ci.yml` is immediately followed by clippy `--all-targets` on the same manifest, and the same pair is in `xtask/src/gates.rs` | drop the `check` step; clippy subsumes it |

`Resource::public_url` (`resource/mod.rs`), `auth::revoke_sessions_for_user`,
`Panel::frame_ancestors`, `ColumnWidth::{Rem, Percent}`, and `Panel::dark_mode` are also uncalled in
this tree, but each is documented app-facing API. They are knobs an app may turn; nothing here turns
them.

## 4. Minor items

| Item | Change |
| --- | --- |
| `filters` URL grammar **[decision]** | `?f.status=published` repeated parameters instead of a nested `key:value,key2:value2` string, which needs its own escaping, two caps, an overflow sentinel, and a JavaScript mirror |
| Route assembly | one `Routes` helper for list, create, edit, delete, bulk, options, and export, consumed by registration and the renderer; `panel/forms/render.rs` recovers the POST target by reading the request path, which `panel/gate.rs` says it does not |
| Confirm dialogs | `render/dialog.rs` and `render/toolbar.rs` each spell Cancel, `confirm=1`, CSRF, and a destructive submit, with two DOM-id formulas |
| Class literals | `class!` constants exist but the render layer repeats the bar class, the quiet link, and `button_variants` as literals |
| `<option>` markup | `schema/fields/select.rs` builds options with `view!`, `panel/actions/options.rs` builds the same markup as a string through a hand-rolled `escape_option`, and `selects.js` mirrors it a third time |
| `ASSET_HOOKS` | one entry per hook, each guarding only that the name appears in its asset and somewhere in Rust. Keep the entries whose two sides can actually differ |
| Doc claims | `docs/dev/architecture.md` says ten browser scripts; there are eleven. Its write-order list documents the `can_*` checks as running before the parse, but delete and bulk delete check `can_view`/`can_delete` inside the transaction after the scoped re-load. `AGENTS.md` calls the two `syn` majors this repository's debt; `tablo-macros` is on `syn` 3 and the `syn` 2 in `Cargo.lock` comes from third-party proc-macro users |

## 5. Upstream dependencies

Both dependencies are workspace members pinned to exact revs, so the question "does a dependency
already do this?" is worth asking of them before crates.io. D1 and D2 are not in the `upstream` issue
register and belong there. #337 and #119 are already filed and cover part of S10 and the projection
closures S2 and S3 work around.

**D1 — An exact `next_cursor`.**

`toasty/src/engine/exec/exec_statement.rs` sets `next_cursor` whenever a page returns a full row
set, with a comment stating the design: the client discovers the end when the next request returns
empty. Tablo cannot accept that, because its last page would be a dead end — the "back to first
page" link renders only when a cursor is present. `load_with_probe` (`table/mod.rs`) therefore
runs a probe query per page in the landing direction, and the tree carries a `BudgetState` tracing
harness plus `full_page_costs_main_plus_single_direction_probe` to prove the probe count. Everything
downstream of "the cursor might not exist" follows: `reject_cursor` (`table/mod.rs`), the
cursor-stripped retry contract, the void-window recovery link, and the tests that pin them.

**Ask.** Make the cursor exact — fetch `page_size + 1` rows internally and report a cursor only when
a further row exists — or expose `has_next: bool` beside it. The SQL path already buffers the page
before it applies pagination, so the extra row is available.

**D2 — A string round-trip for `stmt::Value`. Removes `cursor.rs`.**

`crates/tablo-core/src/cursor.rs` exists because `toasty_core::stmt::Value` derives only `Debug`,
`Default`, `Clone`, and `PartialEq` — no `Serialize`, no `Display`, no byte codec — while
`Paginate::after`/`before` take a `Value` and a URL needs ASCII.

**Ask.** `impl Value { pub fn encode(&self) -> Vec<u8>; pub fn decode(&[u8]) -> Result<Value> }`, or
serde impls, with a documented stability guarantee. No crate substitute exists: the variants are
Toasty-private. A cursor may also be a prefix of the `ORDER BY` (`toasty/src/stmt/paginate.rs`),
so a primary-key cursor is not a substitute.

**D3 — `HrefTarget` for a runtime path.**

`HrefTarget` covers `&'static Path`, `&'static str`, and generated route types
(`topcoat-router/src/href.rs`), but not `PathBuf` or `&Path`. Tablo's routes are runtime
values, so `resource/state.rs` hand-rolls its URL builders, `build_url`, and an escaper instead.
An `impl HrefTarget for &Path` lets those projections build through `.query(..)`, which is what
makes S10 and the `filters` change cheap.

Topcoat's `query_params` (`topcoat-router/src/query_param.rs`) is also unused and would replace
one of the three hand-rolled query-string parsers.

## 6. Do not change

Each of these looks like a simplification until it is read.

- **The security boundaries.** `sanitize_filename`, `is_windows_reserved_name`, and the RFC 5987
  decoder (`panel/forms/decode.rs`) cover a surface no crate in the tree covers; the CSRF
  compare is constant-time; the session transport rides `topcoat::session` with the hardened
  `__Host-` cookie; `infrastructure_failure` keeps driver text out of responses.
- **The tenancy code.** Small, and `tenant_field_index` is used by both the filter builder and the
  build checks.
- **`db.rs`, `commit.rs`, `notification.rs`.** `db(cx)` is called from every loader and handler and
  `Committed` from every mutation. The flash rides Topcoat's `cookie_store`, so it needs no table.
- **`crates/tablo-test`.** Every export is called from both integration suites.
- **The root re-exports that look unused** (`FieldLens`, `IntoSchema`, `NavTarget`, `TablePage`,
  `RowKey`, `FieldErrorKind`). They are names an app writes in a trait bound or signature.
- **The vendored primitives and their xtask sync.** `verify-topcoat-ui` is what makes drift fail the
  build. Do not hand-edit that directory.
- **The client scripts.** Almost all of them are load-bearing or fallbacks. htmx does not replace
  them: Topcoat hydrates once from its own render units, so swapped markup leaves every binding and
  shard marker dead, and there is no HTML-partial endpoint for `hx-get`.
- **The `too_many_lines` budget and the lint-channel `deny(warnings)`.** The workspace manifest
  comment explaining why warnings are denied through the lint table rather than `RUSTFLAGS` is
  correct.
- **`tablo-ui` as a crate.** It is the sync unit for the vendored set, and no `tablo-ui` type crosses
  it: most of the consumed names are `topcoat::asset::Asset` constants.

## 7. Batches

Each batch is independently mergeable and ends with the gate set for what it touched.

1. **Tests.** T1. No behavior change; makes every later review smaller.
2. **Removals.** §3, plus S13, S22, and the doc claims in §4. No redesign.
3. **Mutation path.** S15, S16, S9. Three public names change.
4. **Upstream.** File D1, D2, and D3 with the reproductions the tree already carries as tests.
5. **Guide.** G1, G2.
6. **Declaration.** S1, S3, S4, S5, S6, S7, S8. The largest batch and the largest API change;
   it conflicts with the shape #382 and #383 established, so plan it against those first.
7. **List path.** S10, S11, S12, S14, and the `filters` change if accepted. Easiest once D1-D3
   have answers.
8. **View and infrastructure.** S17-S19, S20, S23, S24, S25, S26.
9. **S21 (CSRF)** on its own, as a security decision.

## 8. Open decisions

1. **Does anything outside this repository consume the public API?** Several items in §3 and §6 are
   only decidable with that answer. If nothing does, delete `frame_ancestors`,
   `ColumnWidth::{Rem, Percent}`, `Panel::dark_mode`, and `Resource::public_url` as well.
2. **Is the `filters` URL grammar change acceptable?** It changes a visible URL shape, so a
   bookmarked filtered list breaks.
3. **Is the CSRF token kept (S21), and if so, what is its rotation story?** It is minted once and
   never rotated, which is defensible only while the layer is documented as redundant.
