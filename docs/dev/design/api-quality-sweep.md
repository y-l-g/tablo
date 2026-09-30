# API quality sweep

Closes #TBD (open the proposal issue per `docs/dev/design/README.md` step 1,
then fill in the number).

## Summary

A verified, ordered list of actionable defects in the write pipeline, the
public API boundary, the gate set, and the docs. Each item states the defect
with a code reference and the spec of the fix. Every claim below was checked
against the tree at `f0d336e5`; sub-claims that did not verify were dropped
and are recorded under Out of scope.

## Motivation

The defects cluster in four places: the write pipeline performs an
irreversible side effect before the authoritative policy check; the public API
has no boundary and is unpublishable as configured; the layering and blast
radius docs contradict the code; and the gate set is slow, serial, and
rewrites the tree before checking it. None of these needs a new feature. Each
needs a small spec reviewers accept once, so implementations do not
re-litigate it.

## Sequence

Order only, earliest first. Items inside a group are independent unless noted.

### 1. Correctness and the guide's front door

**1. Upload bytes persist before the authoritative policy check.**
`panel/forms/submit.rs:297-303` calls `prepare_submission`, which stores
uploads at `:79-80` (`upload::store_uploads`, real uploader writes at
`upload.rs:198`). The advisory pre-check runs at `:291-296`; the transaction
opens at `:307-308` and the authoritative re-check runs at `:310-315`.
A stale-advisory submit writes bytes before the in-tx denial.
Fix: move `store_uploads` out of `prepare_submission` to after the in-tx
check in both create and edit handlers;
`drop_client_typed_uploads`/`restore_pending_uploads` stay where they are.
Add the step to the write flow in `docs/dev/architecture.md:64-77`, which
never mentions uploads.

**2. `Field::relationship` accepts a query scope and discards it.**
`schema/fields/mod.rs:328-338` takes
`query: fn(&Cx) -> Query<List<R::Model>>` and runs `let _ = query;`. Only the
argument value is inert (`R` still constrains the closures and the loader),
but an app that passes a scope believing it filters gets every row.
Fix: delete the parameter and update the call sites to drop the argument
(showcase `app.rs:296,306,668`, `docs/guide/src/forms.md:171`, relation and
options tests). If an inference path needs the type later, it gets a separate
constructor so it cannot be mistaken for configuration.

**3. The guide's only complete example cannot produce a working panel.**
`docs/guide/src/first-panel.md:53-88` has no `#[layout]` calling
`Panel::layout_shell` (`layout_shell` appears in 0 of 11 guide pages and in 4
ADRs). Fix: paste `examples/showcase/src/app.rs:785-787` verbatim into
`first-panel.md`, and replace `:96-98` with a copy-pasteable
`[dependencies]` block carrying the real rev pins and feature list, plus the
stylesheet note (`CONTEXT.md:356-361`: Tablo provides no stylesheet).

### 2. API boundary, layering, gates

**4. The public API has no boundary and is unpublishable as configured.**
361 `pub` items, all crate-root-reachable, no `#[non_exhaustive]`, no sealed
module. `Table::render` (`resource/table/render/core.rs:34`) has zero
production callers (tests only); panel serves `render_with_state`.
`panel::wired_table` is advertised as the third-party seam
(`docs/dev/architecture.md:104`) but has no guide or showcase caller (only a
bench and a test). Publishing fails three ways: versionless path deps
(`crates/tablo-core/Cargo.toml:20-21`), `include_str!("../../../CONTEXT.md")`
escaping the crate root (`lib.rs:6`), and no `repository`/`homepage`
metadata anywhere.
Fix: decide publish vs. git-only; if git-only, `publish = false` on all
three crates. Either way: delete `Table::render` or route panel through it;
land a showcase page using `wired_table`, or make it `pub(crate)` and drop it
from the extension-point table. Then `#[non_exhaustive]` on `Mutation`,
`NavTarget`, `NotificationStatus`, `FieldErrorKind`, and opaque
`TableState`/`TablePage` behind accessors so a new URL param is not breaking.

**5. The one-way layering claim is false, and the blast radius doc is off.**
`resource/relation.rs:113` wires `crate::panel::relation_table`;
`resource/table/render/toolbar.rs:266` uses `crate::panel::table_search`;
`auth.rs:34` imports panel while panel imports auth both ways
(`panel/mod.rs:132`, `panel/build.rs:111`, `panel/gate.rs:25-26`).
`schema/lenses.rs:3-5` claims the `toasty_core` bridge is two modules; 11
production files name `toasty_core`.
Fix: a gate that fails on `crate::panel::` inside `resource/` and `schema/`
code (doc comments excluded); move `relation_table` into
`resource/table/render/`; move `parse_form_body` to a top-level module so
auth stops importing panel; rewrite the `lenses.rs` header to name all 11.

**6. `cargo xtask check` is 15 serialized steps with two traps.**
`xtask/src/gates.rs:446-522` runs 15 `run.run` calls serially (test, clippy,
fmt, detached-bench clippy, MSRV, udeps, doc, mdbook, …). `gates.rs:519`
hard-requires mdbook with no install hint and no skip. `topcoat fmt` has no
`--check`: `gates.rs:133-146` rewrites the tree, then fails on
`git diff --exit-code`, and the hint never says to revert.
Fix: tier it — `check` is test + clippy + fmt + `node --test`;
`check --full` is the dependency-shaped gates (MSRV, udeps, detached
benches, docs), triggered on `Cargo.toml`/`Cargo.lock`/`benchmarks/**`
diffs. Classify mdbook as optional with a skip message. Snapshot
`git diff --name-only` before `topcoat fmt` and print the revert command on
failure. Single-source the locked-rev install pipeline (today in `gates.rs`,
`AGENTS.md`, and `ci.yml`) behind `cargo xtask fmt --install`.

### 3. Structural

**7. Three API taxes on the resource author.**
(a) 16 of 21 `can_*` bodies in the showcase are `true` or forward to
`can_view_any`; no `Policy` value exists (`CONTEXT.md:190` bans a Policy
trait, nothing supplies the value). Fix: `Policy::allow_all()` /
`read_only()` / `deny_all()` with a `Resource::policy()` default of deny.
One line per permissive resource; default-deny and the row predicates stay.
(b) `Table` has no composition; `Schema` does. `IntoColumns` accepts a value
or a tuple up to 8 (`resource/column.rs:343-374`); shared columns are
re-spelled at each `Table::new`. Fix:
`impl<M> IntoColumns<M> for Vec<TextColumn<M>>` and for slices.
(c) `Resource::form` and `Resource::view` duplicate the same fields
(showcase `app.rs:284` vs `:388`, admitted at `:368-372`). Fix: default the
view to `form(cx)` where `HAS_FORM`, plus a `detail_enabled` boolean to turn
the route off; or `Schema::as_view()` (clone with `required` cleared).

**8. The write handlers are copies of one shape.**
`panel/forms/submit.rs:225` vs `:277` share gate, CSRF, `prepare_submission`,
uniqueness, parse, invalid re-render, and commit; the create/edit delta is a
diff, not a parameter (advisory `None` vs `Some`, `create_record` vs
`Posted::new` + `update_record`). `panel/actions/delete.rs:38` vs
`bulk.rs:38` share gate, `can_delete_any`, parse, CSRF, and the confirm
guard, then diverge on single vs `IN`-list delete.
Fix: one `submit_pipeline::<R>` over `enum WriteTarget { Create, Edit }`;
same for `DeleteTarget::{ One, Many }`. (The `list`/`list_live` pair shares
an idiom, not copy-paste; out of scope.)

**9. The benchmark apparatus is sized for a number nothing gates.**
`benchmarks/README.md:4-6`: the axum-maud and leptos comparators are
compile-only stubs; `verify_parity.sh` builds them but runs in no workflow.
`check_lockstep` (`gates.rs:181-209`) compares only the two upstream SHAs,
which are in sync — while registry drift between the two lockfiles is
unguarded (verified: `deadpool` 0.13.1 vs 0.13.0).
Fix: either gate the benchmark (commit a baseline, fail past an agreed
regression) or shrink it (delete the two stubs, move tablo into the main
workspace). The toasty pin stays: its load-bearing status is unverified, so
no unpin is specified here.

**10. The JS/Rust guard misses the riskiest hook.**
`verify_asset_hooks` covers 42 hook names; the JS selects on hooks outside
that list. The unguarded one that matters is `template[data-topcoat-swap]`
(`mutation-submit.js:105`): no `pub const` anywhere in Tablo, zero `.rs`
hits. 3 of 11 assets have no suite (`sidebar.js`, `theme.js`, `variant.js`),
and `docs/dev/TESTING.md:38` overclaims the coverage.
Fix: add `data-topcoat-swap` to `ASSET_HOOKS` (it goes red immediately,
forcing an explicit upstream-internal decision); assert in the showcase
`live_check.rs` that a live-table body contains the template (~10 lines);
correct the `TESTING.md` line. No browser harness: the JS assets stay
dependency-free with HTTP-level coverage.

**11. `docs/dev/design/` contradicts the code and has no index.**
Four content files total 1,149 lines (1,253 with README and template).
`typed-record-form.md:236` specifies the generic `FieldErrors<UserForm>`
signature; the tree has non-generic `FieldErrors` (`form.rs:471`). It also
specifies `FormResource`/`Panel::form_resource`, which the tree no longer
contains (`single-resource-registration.md:10` records the deletion).
Fix: delete `typed-record-form.md` (verify the other three against the tree
on the way past), replace with an index table (design, issue, PR, owning
ADR). Keep `_template.md`.

**12. `_Avoid_` lines have no checker, and one contradicts the code.**
`CONTEXT.md:156` bans `Mutation` under `### Action`; `Mutation` is
`pub enum` (`resource/commit.rs:25`), re-exported (`lib.rs:61`), and used in
the guide (`resources.md:117`). `RowKey`, `FieldLens`, `Sort`, `Cursor` are
exported but appear in no guide page; neither do `layout_shell`,
`OptionSource`, or `Authenticator` (one `Notification` mention total).
Fix: either delete the `_Avoid_` lines or table-drive them in a gate that
fails on a public identifier match. Add one "Extension points" guide page
covering `OptionSource`, `Authenticator`, `Uploader`, `Notification`,
`layout_shell`, and the `wired_table` recipe — it closes six doc gaps at
once.

### 4. Cheap correctness and hygiene, in order

**13. `too_many_lines = 300` may never fire.** Verify the longest production
function, then lower to 150 or delete the deny and keep the rule as prose.
A deny that cannot fire misleads reviewers and agents.

**14. The live shard may read the bulk signal.**
`resource/state.rs:44-46` says a checkbox click must not reload rows, but
`toolbar.rs:102-114` renders `$(bulk.get())` in the live branch, and
`list/tests.rs:336-340` pins related behavior. Fix: add a query-count test
first; then either move the ids transport outside the swapped region or fix
the comment so both sides agree.

**15. Relationship `<select>` order is driver-dependent.**
`schema/relationship.rs:246-253` issues no `ORDER BY`; the `/options`
endpoint 95 lines down applies `R::order_by(cx)` at `:343-350`. Fix: one
`if let` in the base loader.

**16. `TabloError::Declaration` messages reach neither response nor log.**
Constructed at `panel/forms/submit.rs:208-214` and `cursor.rs:71-73`,
returned, then discarded; the enum is `pub(crate)` (`error.rs:12`), so apps
cannot match it. Fix: log in the constructors; test that the message reaches
a tracing subscriber.

**17. `resource/mod.rs:412-414` says "a record fn error is a 500" — it is
passthrough.** `panel/write.rs:46-52` keeps non-toasty mappings, so a
`create_record` returning `not_found()` answers 404 after the failure toast
is already queued (`write.rs:47`). Fix the doc; optionally wrap
non-`Declaration` app errors.

**18. Every list render builds the whole view `Schema`.**
`R::viewed(cx)` (`resource/mod.rs:237-239`) constructs a full `Schema` to
answer a boolean, on every list page, every live-search rerun, and twice on
the detail page (`panel/detail.rs:39` and `:49`). Fix: `fn declares_view(cx)
-> bool`, checked the way table/form declarations already are.

**19. Sequential relationship loads and bulk deletes.**
`schema/tree.rs:181-183` awaits each top-level node in turn; bulk delete
loops per record (`resource/mod.rs:533-536`) with the cap at
`panel/actions/bulk.rs:132` (`MAX_BULK_IDS = 400`). Fix: `join_all` over
top-level nodes; document the bulk cost and the one-statement override.

**20. Checked-in numbers contradict the tree.**
`.cargo/config.toml:4-6`, `docs/adr/0015:9`, `crates/tablo-core/tests/it.rs:1-7`,
and `ci.yml:22-25` (which tells readers to run the `--precise` command
`CONTRIBUTING.md:99-121` forbids) each need a one-line correction in one
commit; re-measure the counts on the way past rather than trusting the
figures quoted here. Extend `pins_match_ci_and_docs`
(`xtask/src/gates/tests.rs:302`) from substrings to parsed vectors and
numbers so the gate checks revs, not just toolchains.

**21. Two xtask guards disagree on locking.**
`xtask/src/lib.rs:137-140` runs `cargo metadata` unlocked while `gates.rs:259-271`
passes `--locked` with a written rationale; a stale lockfile passes one guard
(which heals it) and fails the other. Fix: add `--locked`; memoize the
registry lookup in a `OnceLock`.

Standing: a doc-claims gate. Rule 1 in `AGENTS.md` is honored in prose, yet
stale numbers persist because nothing checks a number. Item 20's extended
test is the first instance; add numeric claims to a gate as they change.

## User-facing API

Before and after for the items above that change app code. Items 1, 5, 6, 8,
9, 10, 11, and 13-21 change behavior, tooling, or docs only; app code
migrates nowhere for them.

Item 2 — the scope argument disappears:

```rust
// Before
Field::relationship::<AuthorResource>(AuthorResource::query, |a| a.id.clone(), |a| a.name.clone())
// After
Field::relationship::<AuthorResource>(|a| a.id.clone(), |a| a.name.clone())
```

Item 4 — the boundary tightens. Exhaustive matches on `Mutation`,
`NavTarget`, `NotificationStatus`, and `FieldErrorKind` gain a wildcard arm;
`TableState`/`TablePage` field reads become accessor calls. A new URL param
is no longer a breaking change.

Item 7a — permissive resources collapse to one line:

```rust
// Before: five trivial can_* functions per resource
// After
fn policy() -> &'static Policy { Policy::allow_all() }
```

Item 7b — shared columns compose instead of repeating:

```rust
// Before: every Table::new re-spells id, created-at, actions (tuple ≤ 8)
// After
table_columns![id_col(), created_at_col(), actions_col()]
```

Item 7c — the detail view defaults to the form declaration:

```rust
// Before: Resource::form and Resource::view written twice, byte-identical
// After: no Resource::view; set detail_enabled = false to turn the route off
```

Item 12 — one guide page replaces six scattered gaps: `OptionSource`,
`Authenticator`, `Uploader`, `Notification`, `layout_shell`, and the
`wired_table` recipe.

Item 3 — `first-panel.md` gains the `#[layout]` shell, the pinned
`[dependencies]` block, and the stylesheet note; no API changes.

## Behavior

Uploads (item 1) become post-authorization: a denied submit writes no bytes,
and the architecture doc names the step. Relationship selects (items 2, 15)
load the same rows in a defined order whether rendered on the form page or
through `/options`. Declaration errors (item 16) are visible in logs while
staying opaque to clients; record-fn errors (item 17) keep their mapping and
the doc says so. List renders (item 18) skip `Schema` construction when no
view is declared. Relationship fan-out (item 19) runs concurrently; bulk
delete keeps the 400 cap with a documented cost. The benchmark (item 9) is
either gated or gone — no third state.

## Edge cases

A forged edit POST against a record the advisory snapshot permits but the
in-tx snapshot denies (item 1) must leave no uploader objects behind. A
caller without access (items 1, 17) sees `forbidden()` or the record fn's own
mapping, never a toast-then-contradiction. Empty and over-cap id lists (item
19) keep current behavior: empty parses to empty, over 400 is rejected before
any delete. Pagination cursors (item 16) keep opaque bodies; only the log
gains the message.

## Alternatives

Publish-all-now instead of item 4's decision-first was discarded: the three
publishing defects make it a multi-crate release project, not a flag flip.
Unpinning toasty to crates.io versions instead of item 9's gate-or-shrink was
discarded: its load-bearing status is unverified, so this doc specifies no
unpin. A full browser harness instead of item 10's assertion was discarded:
cost against 1,804 prod LOC with zero dependencies. Deleting the
`too_many_lines` deny instead of item 13's verify-first was not decided here;
the measurement picks the branch.

## Open questions

Blocking acceptance: publish or git-only (item 4); gate the benchmark or
shrink it (item 9); `_Avoid_` checker or deletion (item 12).
Blocking implementation: view-from-form vs `Schema::as_view()` (item 7c);
wrap non-`Declaration` app errors or doc-only (item 17).
Deferrable: `TableState`/`TablePage` accessor names (item 4); regression
threshold if the benchmark is gated (item 9).

## Out of scope

Deliberately not specified, so reviewers do not expand this doc. Claims that
did not verify on the tree: topcoat-default compression in
`benchmarks/tablo/Cargo.toml` (no `compress` match anywhere); "65 packages
out of sync" (lockstep SHAs match; one registry drift verified, no count);
the pinned toasty being byte-identical to crates.io 0.11.0 (unresolvable
offline — hence no unpin); `Status:`/`Landed:` fields in design docs (no such
convention exists); `with_delete`/`with_edit`/`with_view`/`with_bulk_delete`
being public (all four are `pub(crate)`); "every call site compiles
unchanged" for item 2 (call sites pass the argument and must drop it); the
`list`/`list_live` pair in item 8 (shared idiom, not copy-paste); a fixed
Topcoat 500 body string (no match in the repo). The `REV` install pipeline is
duplicated in two places with pointers elsewhere, not three full copies.
