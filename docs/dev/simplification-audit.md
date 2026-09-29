# Simplification spec

Where the workspace is larger or more intricate than its feature set requires, and the change that
removes the excess. Each item states the current code, the change, and what the change removes.

Baseline: `master` at `8338c329` plus PR #391 (`b887153b`, #383). Every claim cites `path:line`
against that tree. Items marked **[decision]** need a product call rather than a patch.

Scope: every production module in `crates/tablo-core/src` (18,941 non-test lines), plus
`crates/tablo-macros`, `crates/tablo-ui`, `docs/guide/`, `xtask/`, the showcase, and the pinned
Toasty (`b171322`) and Topcoat (`866f81af`) checkouts.

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

**S1 — One schema per resource. ~40 lines, 3 trait methods.**

`Resource::view` (`resource/mod.rs:185`) and `view_values` (`:568`) sit beside `Resource::form`
(`:419`). `view_values` has no override anywhere in the tree, and `panel/detail.rs:48` renders
`view()` from a map the form already hydrates:

```rust
let mut values = R::view_values(cx, record);
values.extend(<R::Form as RecordForm>::hydrate(cx, record));
```

Nothing ties the map's keys to the schema's, so a renamed field renders blank. The showcase repeats
its status and featured selects in both `view` and `form` (`examples/showcase/src/app.rs`).

**Change.** Delete `view`, `view_values`, and `viewed` (`:222`). The detail page renders `form(cx)`
read-only through `R::Form::hydrate`. Add one modifier (`.detail_hidden()`) for the keys a detail
page omits. `viewed` becomes "the form is not empty", so the route, the View link, and the rendered
schema still cannot disagree — now across one schema instead of two.

**Removes.** 3 trait methods, one concept, ~35 showcase lines, and the unchecked view/values drift.

**S2 — Delete `export_query` and the include declaration. ~210 lines, 2 trait methods.**

`export_query` (`resource/mod.rs:361`) defaults to `query_with`, and its only production caller is
`panel/actions/export.rs:264`. `IncludeNeeds` is non-empty at exactly two call sites — the list
(`panel/list.rs:413`) and the export (`export.rs:155`) — and both pass the union of every rendered
column. Every other loader passes `IncludeNeeds::default()`. Because the set cannot be derived, a
wrong name in `TextColumn::needs` is not a compile error: it renders `"(unloaded)"`
(`resource/column.rs:253`) or panics in `Deferred::get`.

**Change.** Keep the narrowing, which is real: option loads must not drag a post's comments into a
comment form. Name it instead of parameterizing it — `fn query(cx)` for the list and detail, `fn
query_record(cx)` for options and probes — and delete `IncludeNeeds`, `TextColumn::needs`,
`include_names`, `Table::include_needs` (`table/mod.rs:344`), `query_with`, `export_query`, and
`scoped_query_with`.

**Removes.** 2 trait methods, 4 concepts, one query per list load, and the `"unloaded"` failure mode.

**S3 — One leaf type. ~370 lines.**

`TextInput`, `Textarea`, `Select`, and `FileUpload` repeat `validate`, the read-only head, and the
`required`/`optional`/`label` builders. Nine constructors resolve a lens and spell a struct literal.
Only `TextInput` and `Textarea` have a `_context` twin (`text_input.rs:151`, `:234`,
`textarea.rs:64`), so an embedded `Select` or `FileUpload` cannot bind at all. The `EmbeddedForm`
derive already treats a textarea as a modifier (`#[form(textarea, rows = N)]`), so the type system
and the macro disagree.

**Change.** `Leaf { name, label, required, rules, control }` with
`Control::{Text { input_type, rows }, Choice { .. }, File }`; one constructor shape per kind;
`.multiline(rows)` instead of a type. Delete `Textarea` and five of the nine constructors.

**Removes.** 4 field types to 1, 9 constructors to 3, and the `_context` asymmetry.

**S4 — Compile the leaf list once. ~150 lines.**

`Schema` re-walks its own tree behind nine accessors: `leaves` (`schema/mod.rs:312`), `any_leaf`
(`:326`), `text_inputs` (`:335`), `select_inputs` (`:343`), `file_uploads` (`:351`),
`has_file_upload` (`:361`), `field_names` (`:260`), `unknown_keys` (`:286`), and `normalize_values`
(`:153`). Each collector re-walks the tree and clones every field it collects, and a create POST
reads several of them.

**Change.** `enum Node { Field(usize), Section(..), Group(..), Grid(..) }` plus
`Schema { nodes, fields: Vec<Field> }`, built in the walk `assert_unique_field_names` (`:298`)
already runs. Keep one `fields()` iterator. Delete the other eight accessors and the four-arm
matches in `schema/tree.rs`.

**Removes.** 9 accessors to 1, and one build pass plus one read instead of repeated walks over
cloned maps.

**S5 — Delete `Tabs`; fold `Repeater` into `Section`. ~230 lines.**

`Tabs` renders a plain stacked `div` and its own doc says so (`schema/layouts.rs:475`). A workspace
grep finds only its own tests and one guide line, so it is public API with no caller. `Repeater`
renders the same border-only panel as `Section`; the only differences are `required` and the
`SkippedBy` split it forces.

**Change.** Delete `Tabs` (the `Node` variant, the macro entry, the guide line). Make
`Section::required()` the titled required group so the repeater branch of `walk_absent_groups` folds
into the generic one.

**Removes.** 5 layout blocks to 3, one `Node` variant, and the `SkippedBy` enum.

**S6 — One render entry, one validate entry. ~75 lines.**

`Schema::render` (`schema/mod.rs:103`) has no production caller; it is `render_with` with two empty
maps. `render_readonly` (`:117`) and `render_with` (`:191`) differ only in the `Mode` flag the walk
already carries, and their production callers are `panel/forms/render.rs:36` and
`panel/detail.rs:77`. Separately, `Select::validate_async` (`schema/fields/select.rs:363`) repeats
`validate` plus `validate_exists`, which `Schema::validate_async` (`:426`) then re-runs per select.

**Change.** `schema.render(cx, &Source::form(values, errors))` and `Source::view(values)`; one
`Field::validate_async`. Delete `render`, `render_readonly`, `render_with`,
`Select::validate_async`, `SkippedBy`, and the empty-map requiredness probe.

**Removes.** 4 render entries to 1, 2 validation entries to 1, and the required message worded once.

**S7 — An embedded value is a schema node. ~450 lines.**

Nine root re-exports exist to hand the derives column names and variant lists at request time:
`value_keys`, `read_embedded`, `write_embedded`, `leaf_key`, `parse_leaf`, `enum_spec`,
`discriminant_select`, `EnumSpec`, `TypedValue`. Their only callers are the two derives and tests.
`EnumSpec` plus `discriminant_select` (`schema/embedded.rs:145`) and `resolve_embedded_value` with
its four collectors (`schema/lenses.rs:235`) rebuild per request what a node could hold once.

**Change.** `Node::Embedded(EmbeddedGroup)` holding the resolved path, its child fields, and — for
an enum — the discriminant `Select` and per-variant `Group`s. Keys come from the node's children,
the discriminant is a child, and variant hiding is a node property. Only
`EmbeddedForm::{read_form, write_form}` survives.

**Removes.** ~200 lines of `lenses.rs`, ~240 of `embedded.rs`, and 9 public items to one trait.

**S8 — One name per resource. 5 shipped strings, ~50 lines.**

`navigation_label()` (`resource/mod.rs:280`) is plural ("Blog Posts") and is used where a singular
belongs: `panel/list.rs:194` and `panel/forms/render.rs:113` render `Create {label}`, and
`submit.rs:289`, `:364`, and `common.rs:220` render `Edit {label}`. The showcase therefore serves
"Create Blog Posts" and "Edit Blog Posts". `slug()` (`:269`) derives from the resource type while the
label derives from the model type, so `StaffResource` over `User` is `/staff` labelled "Users".

**Change.** Add `fn label() -> String`, singular, defaulted from the model type name; derive
`navigation_label()` as its plural; use `label()` at the five title sites. A wrong plural then costs
one override, so `pluralize`'s irregular, f-exception, and uncountable tables
(`resource/naming.rs:24`) can be deleted.

**Removes.** 5 wrong strings, ~50 lines of naming tables, and the two-source naming split.

**S9 — A check hook replaces the write delegation pair. 4 public names.**

`create_record` (`resource/mod.rs:436`) and `update_record` (`:450`) default to
`write_create`/`write_update` (`form.rs:498`, `:522`). A resource that needs a check inside the
transaction must override the whole method and end with the free function
(`docs/guide/src/forms.md:78`), because Rust cannot call a default from an override. The showcase
does this four times.

**Change.** `fn check_create(cx, &Self::Form, ex) -> Result<()>` and `check_update(cx,
&Posted<Self::Form>, ex)`, default `Ok(())`, called by the handler inside the transaction after the
parse and the unique probe. Remove `create_record`, `update_record`, `write_create`, and
`write_update` from the public surface.

**Removes.** 4 public names to 2, one layer per create, and ~45 lines of `form.rs` doc and body.

### 2.2 List path

**S10 — The URL is the only list state. ~250-350 lines.**

`TableSignals` (`resource/state.rs:37`) carries seven `Signal<String>` fields. One keystroke walks
nine hops: the debounce, a hidden transport that writes `q` and the cursor, the signal, a shard
invocation packing seven handles, the shard's eight wire arguments, the registry, `to_state`,
`normalize_state`, and `load_table_page`, which applies the declaration twice. Every control already
renders the complete URL, so each click re-derives by hand what its `href` states, in three
vocabularies: URL parameter, signal field, and the `after:`/`before:` wire.

**Change.** `TableSignals` becomes one `Signal<String>` holding the list's query string. A control
writes `state.query_string()`; the shard takes `(path, url)` and calls `TableState::from_query`.
Delete `from_live_args` (`state.rs:559`), the four cursor-wire helpers, `TableSearchArgs`, and the
per-field arms of `to_signals`/`to_state`. Upstream, #337 asks Topcoat for the struct-typed shard
signal this needs, and names the same retirement condition.

**Removes.** 4 concepts, and the "GET and live must agree" test because one parse path serves both.

**S11 — One cursor type; delete the `_normalized` twins. ~90 lines.**

`TableState` carries `after` and `before` separately, the `after:<token>` wire, and the cursor token.
Toasty accepts exactly one cursor, so the pair is unrepresentable — the code says so
(`state.rs:44`) and keeps the pair anyway. `NormalizedState` (`table/mod.rs:110`) then doubles every
render seam: `render_with_state` and `render_normalized` (`render/core.rs:124`, `:142`),
`render_live_with_state` and `render_live_normalized` (`:167`, `:185`), and the same pair on the
skeleton (`skeleton.rs:15`, `:29`), the delete dialog (`dialog.rs:37`, `:49`), the live search bar
(`toolbar.rs:257`, `:273`), and the live filter bar (`filterbar.rs:120`, `:139`). Each public half
calls the private half after one clone of a small map.

**Change.** `cursor: Option<Cursor>` with `After`/`Before`. Make the normalizer return `TableState`
and delete the seven `_normalized` methods.

**Removes.** 5 cursor representations to 2, and halves the table render surface.

**S12 — Split the table loader out of `Table`. ~60 lines deleted, ~200 moved.**

`Table<M>` holds 14 declaration fields beside `apply_declaration`, `load` (`table/mod.rs:753`), and
`load_with_probe` (`:770`). `pub Table::load` has no production caller: every caller in the
workspace is a test, and `benchmarks/tablo/src/main.rs:246` re-implements the pipeline because the
real loader is `pub(crate)`. Once `pub Table::load` goes, the unpaginated branch of `load_with_probe` becomes
unreachable: the panel refuses a table with no page size (`panel/list.rs:404`), so every remaining
caller paginates.

**Change.** `Table` keeps the declaration and pure planning, with no `Cx` and no `Db`. Loading moves
beside its only production caller and is published as one value, `ListPage::<R>::load(cx)`, so the
list, the shard, a page-owned table, and the benchmark all call the real loader. Delete
`pub Table::load` and the unreachable branch.

**Removes.** One public method, ~25 hand-mirrored benchmark lines, and makes `Table` assertable
without a `Db`.

**S13 — `paginate(NonZeroUsize)`. ~30 source and ~50 test lines.**

`Table::paginate(per_page: usize)` (`table/mod.rs:478`) documents zero as a programmer error, then
guards it four times: `table/mod.rs:780`, `:883` (`missing_essentials`), `render/core.rs:211`, and
`panel/build.rs:393`.

**Change.** Take `NonZeroUsize`. Delete `missing_essentials` and the build branch.

**Removes.** Four guards, one build-error variant, and three tests.

**S14 — One pager for the list and the export. ~70 lines.**

`ExportChunker` (`table/export.rs:282`) and the paginated branch of `load_with_probe` both build
`Paginate`, decode cursors, re-derive "no cursor means end", and restate Toasty's upper-bound
semantics (`export.rs:275` and `table/mod.rs:825`).

**Change.** One `Pager { query, after, done }` with `next(&mut db, take)` beside the loader; the
export wraps it for its cap and window accounting.

**Removes.** One of the two places that encode Toasty's cursor semantics.

### 2.3 Mutation path

**S15 — One commit tail. ~90-110 lines.**

`commit_write` (`panel/forms/submit.rs:218`) is the shared tail: commit, `run_after_commit`, notify,
redirect. Create and edit use it (`:302`, `:375`). Delete and bulk delete re-implement the same body
(`panel/actions/delete.rs:93-102`, `bulk.rs:118-132`), including the flash and `see_other` sequence that
`redirect_after_write` (`forms/common.rs:200`) already provides. `docs/dev/architecture.md:60`
documents one shape.

**Change.** Move `commit_write` to a shared module and widen it over `Result<T, _>` with
`FnOnce(T) -> Committed<R::Model>`; call it from all four handlers. In the same change, take
`&Model` and `&[Model]` in `delete_record` (`resource/mod.rs:482`) and `bulk_delete_records`
(`:506`) so the handlers stop cloning a pre-delete snapshot, and drop the `Clone` bound from
`Resource::Model`.

**Removes.** Three tails to one, ~90-110 handler lines, and `Clone` from the `Resource::Model`
contract.

**S16 — A working `delete_record` default. ~35 lines.**

`docs/guide/src/resources.md:44` documents that `delete_record` "defaults to an error naming the
type", and no guide chapter shows an implementation. The showcase therefore defines
`delete_through_query!` (`examples/showcase/src/app.rs:56`) and uses it four times (`:205`, `:284`,
`:655`, `:913`). Its whole body is the obvious default:
`Self::query(cx).filter(Model::fields().id().eq(record.id)).delete()`.

Policy already gates reachability, so a resource that never considered delete cannot reach the stub:
`can_delete_any` defaults to `false` and the handler answers 403 before reading the body
(`panel/actions/delete.rs:47`). The stub only produces a resource that passes every build check and
fails on each click.

**Change.** Default `delete_record` to the query-scoped delete. Keep the error only for a model with
no PK-addressable route.

**Removes.** One app-side macro and its four invocations, and one 500-at-click failure mode.

### 2.4 View layer

**S17 — The skeleton derives its chrome from the loaded table. ~25 lines.**

`render/core.rs:480` and `render/skeleton.rs:56` spell the same root `div`, and `core.rs:504` and
`skeleton.rs:101` the same boundary `div`. The skeleton's toolbar pulse (`skeleton.rs:60`) and pager
pulse (`:95`) render unconditionally, while the loaded table gates the search bar on
`search_enabled()`, the filter bar on `filter_bar_enabled()`, and the pager on pagination. A table
with no searchable column shows a search pulse that disappears on swap, and the showcase Post table
renders one bar before the swap and two after it. The guard test compares only the `<table>` opening
tag, so it cannot see the drift.

**Change.** One `table_root`/`table_boundary` used by both, with each pulse gated on the predicate
the loaded table uses.

**Removes.** One class of unchecked drift and a visible layout jump on every list load.

**S18 — One live-or-plain link helper. ~80 lines.**

`render/core.rs:695`, `:729`, `render/filterbar.rs:313`, and `render/pager.rs:45`, `:63` each repeat
the same `match signals { Some(s) => attributes! { href=(url) @click=$(..) }, None => attributes! {
href=(url) } }`.

**Change.** One `live_or_plain(cx, url, signals, write) -> Attributes`.

**Removes.** The fallback and the cursor reset get one edit site instead of five.

**S19 — Split `render_inner`. 0 lines deleted, ~650 moved.**

`render_inner` (`render/core.rs:200-505`) is 306 lines that inline the bulk cell, the cell loop, the
row action menu, the empty-versus-rows branch, the revision signal, and the chrome wrapper. The
table's error state already lives elsewhere (`panel/list.rs:126`), so the function is eight jobs with no
boundary between them.

**Change.** Extract `render_row` and `render_actions`, and split `render/head.rs`, `rows.rs`,
`widths.rs`, and `empty.rs`, the shape `toolbar`, `filterbar`, `pager`, `dialog`, and `skeleton`
already use.

**Removes.** A 306-line function to about 120, and four concerns become files.

### 2.5 Infrastructure

**S20 — Delete the `auth` cargo feature. ~84 lines, 53 cfg sites.**

`crates/tablo-core/Cargo.toml` gates `argon2`, `password-hash`, and `topcoat/session` behind
`auth`. `auth_off.rs` (24 lines) is an `Auth` whose `is_disabled()` returns `true`, which is what
`Auth::disabled()` does. Runtime behavior is identical either way; what differs is the public API,
which lacks `Auth::password`, `Auth::custom`, `hash_password`, `AdminUser`, `AuthSession`, and
`Panel::login_hint` when the feature is off. No workspace member builds it off: the only
`default-features = false` consumer is `tablo-test`, which then mirrors an empty `auth = []` feature
and documents that it does so to avoid Cargo feature unification
(`crates/tablo-test/Cargo.toml:15`).

**Change.** Delete the feature and `auth_off.rs`. Keep `Auth::disabled()` as ADR-0013's explicit
opt-out. `argon2`, `password-hash`, and `session` become unconditional; every default build already
resolves them.

**Removes.** One parallel public API, 53 cfg sites, one Cargo-unification workaround, and the
three-crate optionality that buys nothing.

**S21 — Delete `csrf.rs`. [decision] ~179 lines.**

`csrf.rs:43` reuses any valid existing cookie and mints only when absent: no rotation, no expiry of
its own, and no session binding. Two upstream layers already refuse the attack. Topcoat installs
`origin: OriginLayer::new(self.origin_policy)` unconditionally
(`topcoat-router/src/builder.rs:513`), and the default policy verifies every state-changing request,
rejecting a cross-site `Sec-Fetch-Site` or a mismatched `Origin`/`Host` with 403
(`topcoat-router/src/origin.rs:141`). The session cookie is forced `SameSite=Lax`
(`topcoat-session/src/token/store.rs:45`), so a cross-site POST carries no session. Tablo never
loosens either.

**Change.** Delete `csrf.rs`, the hidden field, and the verify calls; amend ADR-0013 and ADR-0021.

**Decision.** This trades a layer Tablo owns for two upstream layers it does not control. If a
downstream app installs its own router or strips `Origin` and `Sec-Fetch-Site`, the token is the only
remaining defense. Keep it only with documentation that says the protection is redundant on the
shipped router.

**S22 — Method-scope the login gate bypass. 1 line.**

`auth.rs:601` returns unauthenticated for any method at `{prefix}/login`, while the logout bypass two
lines later is deliberately scoped to POST (`:615`) with a comment explaining why. An app route at
`{prefix}/login` under PUT or DELETE runs unauthenticated.

**Change.** `&& matches!(*method(cx), GET | POST)`.

**S23 — Delete the upload carry protocol. ~90 lines, 1 public trait method.**

`Uploader::holds` (`upload.rs:68`), `restore_pending_uploads` (`forms/common.rs:142`), the
`clear_`/`keep_` prefixes (`common.rs:51`), the hidden control (`forms/render.rs:52`), and a
`carried` argument threaded through the form pipeline exist so a file survives a validation
re-render. The only implementation scans the upload directory on every re-render
(`examples/showcase/src/app.rs:1019`).

**Change.** Drop `holds`, `keep_<field>`, and `carried`. A failed create re-renders with an empty
file input; an edit keeps the stored value, which is the existing untouched-field rule.

**Removes.** One directory scan per re-render, one hidden control, and one public trait method.

**S24 — One error vocabulary. ~100 lines.**

Three vocabularies describe one re-render path: `FieldError`/`FieldErrorKind` (`form.rs:194`, `:203`),
`FieldErrors<F>` (`:430`), and `HashMap<String, Vec<String>>` (`schema/mod.rs:377`), merged by hand
in `submit.rs:160`. `db.rs:57` (`hook_failure`) and `auth.rs:202` (`infrastructure_failure`) are the
same is-toasty-then-map check. And 42 non-test sites build domain errors as
`std::io::Error::other(format!(..))` — cursor decode, a table declaration, a missing record fn —
which are then classified by downcast (`cursor.rs:119`, `table/mod.rs:924`).

**Change.** One keyed `FieldErrors` carrying `(key, message, required)`. One
`unavailable_with(error, || ..)` for both infrastructure mappings. A crate `TabloError` enum
(`Cursor`, `CursorRejected`, `Declaration`, `Stub`, `Infrastructure`) with one conversion into
`topcoat::Error`.

**Removes.** Two duplicate vocabularies, one duplicate mapping, and an error taxonomy that is
exhaustive instead of three predicates that must each remember to downcast.

**S25 — Delete the `OptionSource` shim; share the relation column machinery. ~290 lines.**

`OptionSource` (`schema/relationship.rs:41`) plus its blanket impl (`resource/mod.rs:676`) forwards
`can_view_any`, `can_view`, `requires_tenant`, and `slug` unchanged and derives `search_expr` and
`order_by` from `R::table()`. It exists so `schema` does not depend on `resource`, for one
implementor that is not a `Resource` (`examples/showcase/src/media.rs`). Separately,
`resource/relation.rs` re-implements column machinery it should share: `WIDE_COLUMN_MIN_REM`
(`:47`) duplicates `render/core.rs:31`, `relation_widths` (`:197`) re-derives `core.rs:860`, and
`into_relation_columns_tuples!` (`:155`) duplicates `column.rs:406`.

**Change.** Make the one non-`Resource` implementor a `Resource`, or take the trait as a parameter,
and delete the shim. Share the width arithmetic and the tuple macro.

**Removes.** 2 concepts and the divergence between the two column implementations.

### 2.6 Macros

**S26 — One field classifier for both derives. ~150 lines.**

`tablo-macros/src/embedded.rs:27` hardcodes 18 leaf type names, duplicating the 17 `TypedValue`
impls in `schema/validation.rs:53`, with no test linking the two lists. `record_form.rs:234` treats
every non-`embed` field as a scalar and never classifies it, so a `Vec<String>` form field reaches
rustc as an E0277 on generated tokens with no field span. Each record-form field also emits and
resolves its path and key three times (`record_form.rs:250`, `:256`, `:263`), and a three-field
embedded struct emits two `chained(...)` paths per leaf.

**Change.** One `tablo-macros/src/fields.rs` with `enum FieldKind { Scalar, Embedded, Other }`
derived from one table, plus shared attribute parsing. Both derives emit a field-spanned refusal
naming both fixes. Bind each field's key and path once per body.

**Removes.** ~150 of the ~1,200 non-test macro lines, two duplicated walkers, and one E0277 that
names neither the field nor the fix.

### 2.7 Tests

**T1 — Move inline test modules beside their subjects. 17,973 lines relocated, 0 deleted.**

17,973 of `crates/tablo-core/src`'s 36,914 lines (48%) are `#[cfg(test)] mod tests` blocks. The
largest production file is 1,120 lines, not 2,233. `panel/list.rs` is 78% tests,
`panel/actions/export.rs` 73%, `panel/build.rs` 65%.

**Change.** `#[cfg(test)] mod tests;` in a sibling `tests.rs` per module, reusing the fixtures that
already exist (`panel/test_support.rs`, `test_support.rs`, `tests/common/mod.rs`). Add a gate on
production lines per file so the `too_many_lines` budget measures code rather than tests.

**Removes.** Nothing. It makes every source file about half as long to read and makes the
production-only outliers visible.

### 2.8 Guide

**G1 — A hello-world chapter.**

The guide never shows `Panel::build`'s required `app_context` (`panel/build.rs:90`) outside prose,
and `docs/guide/src/testing-and-benchmarks.md:5` points at `CxTestBuilder` — a `topcoat::context`
type `tablo-core` does not re-export — while never naming `crates/tablo-test`. Both gaps stop an
author before the first line of real code.

**Change.** One chapter with the ~30-line `main`, and `tablo_test::TestClient` in the testing
chapter.

**G2 — A document constructor for public pages.**

`docs/guide/src/panel-and-routing.md:58` shows a 22-line document that duplicates the panel's head,
and `:89` explains the `try_app_context::<AssetConfig>(cx).is_some()` guard a markup test needs.

**Change.** A public document or layout constructor on `Panel` that reuses the shell's head and its
asset fallback.

## 3. Removal list

Items with no caller in `crates/`, `examples/`, `benchmarks/`, or `xtask/`, each verified by grep.
The individual sizes are small; the value is the smaller public surface.

| Item | Evidence | Action |
| --- | --- | --- |
| `Posted::into_form` | `form.rs:349` — the definition is the only occurrence; `Posted` already `Deref`s to `F` | delete |
| `Table::render_delete_dialog` | `render/dialog.rs:37` — no caller; `:49` is live | delete |
| `Table::render_live_filter_bar` | `render/filterbar.rs:120` — no caller; `:139` is live | delete |
| `Table::render_live_search_bar` | `render/toolbar.rs:257` — no caller; `:273` is live; its doc claims a showcase caller that does not exist | delete and fix the doc |
| `Table::render_live_with_state` | `render/core.rs:167` — no caller; `:184` is live | delete |
| `Table::render_skeleton` | `render/skeleton.rs:15` — called only by its own tests | make `pub(crate)` |
| `Table::actions_for` | `table/mod.rs:493` — one in-crate caller (`render/core.rs`) | make `pub(crate)` |
| `GroupDef` re-export | `table/mod.rs:75` — private fields, no `impl`, built only inside `group_by` | drop from the re-export |
| `RowActions` re-export | `lib.rs:56` — its only setter is `pub(crate)` | drop, or open the seam |
| `live_search(bool)` argument | every call site passes `true`; the default is already `false` | `live_search()` |
| `search(true)` / `filter_bar(true)` arms | never called; only `search(false).filter_bar(false)` is | `hide_search()` / `hide_filter_bar()` |
| 29 `tablo-ui` re-exports | `tablo-ui/src/lib.rs:25-59` — no reference outside `primitives/` | delete; the sync guard still passes |
| CI bench `cargo check` | `ci.yml:182` is immediately followed by clippy `--all-targets` on the same manifest (`:186`), and the same pair is in `xtask/src/gates.rs` | drop the `check` step; clippy subsumes it |

`Resource::public_url` (`resource/mod.rs:256`), `auth::revoke_sessions_for_user`,
`Panel::frame_ancestors`, `ColumnWidth::{Rem, Percent}`, and `Panel::dark_mode` are also uncalled in
this tree, but each is documented app-facing API. They are knobs an app may turn; nothing here turns
them.

## 4. Minor items

| Item | Change | Size |
| --- | --- | --- |
| `filters` URL grammar **[decision]** | `?f.status=published` repeated parameters instead of a nested `key:value,key2:value2` string, which needs its own escaping, two caps, an overflow sentinel, and a JavaScript mirror | ~180 Rust, ~10 JS |
| Route assembly | one `Routes` helper for list, create, edit, delete, bulk, options, and export, consumed by registration and the renderer; `panel/forms/render.rs:37` recovers the POST target by reading the request path, which `panel/gate.rs:11` says it does not | ~40 |
| Confirm dialogs | `render/dialog.rs:104` and `render/toolbar.rs:150` each spell Cancel, `confirm=1`, CSRF, and a destructive submit, with two DOM-id formulas | ~40 |
| Class literals | `class!` constants exist but the render layer repeats the bar class three times, the quiet link four, and `button_variants` five | ~60 |
| `<option>` markup | `schema/fields/select.rs:505` builds options with `view!`, `panel/actions/options.rs:58` builds the same markup as a string through a hand-rolled `escape_option` (`:119`), and `selects.js` mirrors it a third time | ~14 |
| `ASSET_HOOKS` | 223 lines and 41 entries guard that a hook appears in its asset and somewhere in Rust. Keep the entries whose two sides can actually differ | ~150 tooling lines |
| Doc claims | `docs/dev/architecture.md:120` says ten browser scripts; there are eleven. `:64` documents the `can_*` checks as running before the parse, but delete and bulk delete check `can_view`/`can_delete` inside the transaction after the scoped re-load. `AGENTS.md` calls the two `syn` majors this repository's debt; `tablo-macros` is on `syn` 3 and the `syn` 2 in `Cargo.lock` comes from third-party proc-macro users | 3 claims |

## 5. Upstream dependencies

Both dependencies are workspace members pinned to exact revs, so the question "does a dependency
already do this?" is worth asking of them before crates.io. D1 and D2 are not in the `upstream` issue
register and belong there. #337 and #119 are already filed and cover part of S10 and the projection
closures S2 and S3 work around.

**D1 — An exact `next_cursor`. Removes ~600 lines and one query per page.**

`toasty/src/engine/exec/exec_statement.rs:226` sets `next_cursor` whenever a page returns a full row
set, with a comment stating the design: the client discovers the end when the next request returns
empty. Tablo cannot accept that, because its last page would be a dead end — the "back to first
page" link renders only when a cursor is present. `load_with_probe` (`table/mod.rs:770`) therefore
runs a probe query per page in the landing direction, and the tree carries a `BudgetState` tracing
harness plus `full_page_costs_main_plus_single_direction_probe` to prove the probe count. Everything
downstream of "the cursor might not exist" follows: `reject_cursor` (`table/mod.rs:924`), the
cursor-stripped retry contract, the void-window recovery link, and about thirteen tests.

**Ask.** Make the cursor exact — fetch `page_size + 1` rows internally and report a cursor only when
a further row exists — or expose `has_next: bool` beside it. The SQL path already buffers the page
(`:198`), so the extra row is available.

**D2 — A string round-trip for `stmt::Value`. Removes `cursor.rs`.**

`crates/tablo-core/src/cursor.rs` exists because `toasty_core::stmt::Value` derives only `Debug`,
`Default`, `Clone`, and `PartialEq` — no `Serialize`, no `Display`, no byte codec — while
`Paginate::after`/`before` take a `Value` and a URL needs ASCII.

**Ask.** `impl Value { pub fn encode(&self) -> Vec<u8>; pub fn decode(&[u8]) -> Result<Value> }`, or
serde impls, with a documented stability guarantee. No crate substitute exists: the variants are
Toasty-private. A cursor may also be a prefix of the `ORDER BY`
(`toasty/src/stmt/paginate.rs:114`), so a primary-key cursor is not a substitute.

**D3 — `HrefTarget` for a runtime path.**

`HrefTarget` covers `&'static Path`, `&'static str`, and generated route types
(`topcoat-router/src/href.rs:24`, `:35`), but not `PathBuf` or `&Path`. Tablo's routes are runtime
values, so `resource/state.rs` hand-rolls twelve URL builders, `build_url`, and an escaper instead.
An `impl HrefTarget for &Path` lets those projections build through `.query(..)`, which is what
makes S10 and the `filters` change cheap.

Topcoat's `query_params` (`topcoat-router/src/query_param.rs:32`) is also unused and would replace
one of the three hand-rolled query-string parsers.

## 6. Do not change

Each of these looks like a simplification until it is read.

- **The security boundaries.** `sanitize_filename`, `is_windows_reserved_name`, and the RFC 5987
  decoder (`panel/forms/decode.rs:264`) cover a surface no crate in the tree covers; the CSRF
  compare is constant-time; the session transport rides `topcoat::session` with the hardened
  `__Host-` cookie; `infrastructure_failure` keeps driver text out of responses.
- **The tenancy code.** 120 production lines, and `tenant_field_index` is used by both the filter
  builder and the build checks.
- **`db.rs`, `commit.rs`, `notification.rs`.** 34 `db(cx)` call sites and 21 `Committed` uses. The
  flash rides Topcoat's `cookie_store`, so it needs no table.
- **`crates/tablo-test`.** 278 lines; all eleven exports are called from both integration suites.
- **The root re-exports that look unused** (`FieldLens`, `IntoSchema`, `NavTarget`, `TablePage`,
  `RowKey`, `FieldErrorKind`). They are names an app writes in a trait bound or signature.
- **The vendored primitives and their xtask sync.** `verify-topcoat-ui` is what makes drift fail the
  build. Do not hand-edit that directory.
- **The client scripts.** Ten of the eleven are load-bearing or fallbacks. htmx does not replace
  them: Topcoat hydrates once from its own render units, so swapped markup leaves every binding and
  shard marker dead, and there is no HTML-partial endpoint for `hx-get`.
- **The `too_many_lines` budget and the lint-channel `deny(warnings)`.** The workspace manifest
  comment explaining why warnings are denied through the lint table rather than `RUSTFLAGS` is
  correct.
- **`tablo-ui` as a crate.** It is the sync unit for the vendored set, and no `tablo-ui` type crosses
  it: 11 of the 32 consumed names are `topcoat::asset::Asset` constants.

## 7. Batches

Each batch is independently mergeable and ends with the gate set for what it touched.

1. **Tests.** T1. No behavior change; makes every later review smaller.
2. **Removals.** §3, plus S13, S22, and the doc claims in §4. About 400 lines, no redesign.
3. **Mutation path.** S15, S16, S9. About 250 lines and three public names.
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
