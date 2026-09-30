# API quality sweep

Closes #TBD (open the proposal issue per `docs/dev/design/README.md` step 1,
then fill in the number).

Ordered findings, earliest first. Each item states the defect with a code
reference and the spec of the fix.

1. Upload bytes persist before the authoritative policy check.
`panel/forms/submit.rs:297-303` calls `prepare_submission`, which stores
uploads at `:79-80` (`upload::store_uploads`, real uploader writes at
`upload.rs:198`). The advisory pre-check runs at `:291-296`; the transaction
opens at `:307-308` and the authoritative re-check runs at `:310-315`. A
stale-advisory submit writes bytes before the in-tx denial.
Fix: move `store_uploads` out of `prepare_submission` to after the in-tx
check in both create and edit handlers;
`drop_client_typed_uploads`/`restore_pending_uploads` stay where they are.
Add the step to the write flow in `docs/dev/architecture.md:64-77`, which
never mentions uploads.

2. `Field::relationship` accepts a query scope and discards it.
`schema/fields/mod.rs:328-338` takes
`query: fn(&Cx) -> Query<List<R::Model>>` and runs `let _ = query;`. Only the
argument value is inert, but an app that passes a scope believing it filters
gets every row.
Fix: delete the parameter; call sites drop the argument (showcase
`app.rs:296,306,668`, `docs/guide/src/forms.md:171`, relation and options
tests):

```rust
Field::relationship::<AuthorResource>(|a| a.id.clone(), |a| a.name.clone())
```

3. The guide's only complete example cannot produce a working panel.
`docs/guide/src/first-panel.md:53-88` has no `#[layout]` calling
`Panel::layout_shell` (`layout_shell` appears in 0 of 11 guide pages and in 4
ADRs).
Fix: paste `examples/showcase/src/app.rs:785-787` verbatim into
`first-panel.md`, and replace `:96-98` with a copy-pasteable
`[dependencies]` block carrying the real rev pins and feature list, plus the
stylesheet note (`CONTEXT.md:356-361`: Tablo provides no stylesheet).

4. The public API has no boundary and is unpublishable as configured. 361
`pub` items, all crate-root-reachable, no `#[non_exhaustive]`, no sealed
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

5. The one-way layering claim is false, and the blast radius doc is off.
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

6. `cargo xtask check` is 15 serialized steps with two traps.
`xtask/src/gates.rs:446-522` runs 15 `run.run` calls serially. `gates.rs:519`
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

7. Three API taxes on the resource author. (a) 16 of 21 `can_*` bodies in the
showcase are `true` or forward to `can_view_any`; no `Policy` value exists
(`CONTEXT.md:190` bans a Policy trait, nothing supplies the value).
Fix: `Policy::allow_all()` / `read_only()` / `deny_all()` with a
`Resource::policy()` default of deny:

```rust
fn policy() -> &'static Policy { Policy::allow_all() }
```

(b) `Table` has no composition; `Schema` does. `IntoColumns` accepts a value
or a tuple up to 8 (`resource/column.rs:343-374`); shared columns are
re-spelled at each `Table::new`.
Fix: `impl<M> IntoColumns<M> for Vec<TextColumn<M>>` and for slices.
(c) `Resource::form` and `Resource::view` duplicate the same fields
(showcase `app.rs:284` vs `:388`, admitted at `:368-372`).
Fix: default the view to `form(cx)` where `HAS_FORM`, plus a
`detail_enabled` boolean to turn the route off; or `Schema::as_view()`
(clone with `required` cleared).

8. The write handlers are copies of one shape.
`panel/forms/submit.rs:225` vs `:277` share gate, CSRF, `prepare_submission`,
uniqueness, parse, invalid re-render, and commit; the create/edit delta is a
diff, not a parameter. `panel/actions/delete.rs:38` vs `bulk.rs:38` share
gate, `can_delete_any`, parse, CSRF, and the confirm guard, then diverge on
single vs `IN`-list delete.
Fix: one `submit_pipeline::<R>` over `enum WriteTarget { Create, Edit }`;
same for `DeleteTarget::{ One, Many }`.

9. The benchmark apparatus is sized for a number nothing gates.
`benchmarks/README.md:4-6`: the axum-maud and leptos comparators are
compile-only stubs; `verify_parity.sh` builds them but runs in no workflow.
`check_lockstep` (`gates.rs:181-209`) compares only the two upstream SHAs,
which are in sync — while registry drift between the two lockfiles is
unguarded (verified: `deadpool` 0.13.1 vs 0.13.0).
Fix: either gate the benchmark (commit a baseline, fail past an agreed
regression) or shrink it (delete the two stubs, move tablo into the main
workspace).

10. The JS/Rust guard misses the riskiest hook. `verify_asset_hooks` covers
42 hook names; the JS selects on hooks outside that list. The unguarded one
that matters is `template[data-topcoat-swap]` (`mutation-submit.js:105`): no
`pub const` anywhere in Tablo, zero `.rs` hits. 3 of 11 assets have no suite
(`sidebar.js`, `theme.js`, `variant.js`), and `docs/dev/TESTING.md:38`
overclaims the coverage.
Fix: add `data-topcoat-swap` to `ASSET_HOOKS` (it goes red immediately,
forcing an explicit upstream-internal decision); assert in the showcase
`live_check.rs` that a live-table body contains the template (~10 lines);
correct the `TESTING.md` line. No browser harness: the JS assets stay
dependency-free with HTTP-level coverage.

11. `docs/dev/design/` contradicts the code and has no index. Four content
files total 1,149 lines (1,253 with README and template).
`typed-record-form.md:236` specifies the generic `FieldErrors<UserForm>`
signature; the tree has non-generic `FieldErrors` (`form.rs:471`). It also
specifies `FormResource`/`Panel::form_resource`, which the tree no longer
contains (`single-resource-registration.md:10` records the deletion).
Fix: delete `typed-record-form.md` (verify the other three against the tree
on the way past), replace with an index table (design, issue, PR, owning
ADR). Keep `_template.md`.

12. `_Avoid_` lines have no checker, and one contradicts the code.
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

13. `too_many_lines = 300` may never fire. Verify the longest production
function, then lower to 150 or delete the deny and keep the rule as prose.

14. The live shard may read the bulk signal. `resource/state.rs:44-46` says a
checkbox click must not reload rows, but `toolbar.rs:102-114` renders
`$(bulk.get())` in the live branch, and `list/tests.rs:336-340` pins related
behavior.
Fix: add a query-count test first; then either move the ids transport outside
the swapped region or fix the comment so both sides agree.

15. Relationship `<select>` order is driver-dependent.
`schema/relationship.rs:246-253` issues no `ORDER BY`; the `/options`
endpoint 95 lines down applies `R::order_by(cx)` at `:343-350`.
Fix: one `if let` in the base loader.

16. `TabloError::Declaration` messages reach neither response nor log.
Constructed at `panel/forms/submit.rs:208-214` and `cursor.rs:71-73`,
returned, then discarded; the enum is `pub(crate)` (`error.rs:12`), so apps
cannot match it.
Fix: log in the constructors; test that the message reaches a tracing
subscriber.

17. `resource/mod.rs:412-414` says "a record fn error is a 500" — it is
passthrough. `panel/write.rs:46-52` keeps non-toasty mappings, so a
`create_record` returning `not_found()` answers 404 after the failure toast
is already queued (`write.rs:47`).
Fix the doc; optionally wrap non-`Declaration` app errors.

18. Every list render builds the whole view `Schema`. `R::viewed(cx)`
(`resource/mod.rs:237-239`) constructs a full `Schema` to answer a boolean,
on every list page, every live-search rerun, and twice on the detail page
(`panel/detail.rs:39` and `:49`).
Fix: `fn declares_view(cx) -> bool`, checked the way table/form declarations
already are.

19. Sequential relationship loads and bulk deletes.
`schema/tree.rs:181-183` awaits each top-level node in turn; bulk delete
loops per record (`resource/mod.rs:533-536`) with the cap at
`panel/actions/bulk.rs:132` (`MAX_BULK_IDS = 400`).
Fix: `join_all` over top-level nodes; document the bulk cost and the
one-statement override.

20. Checked-in numbers contradict the tree. `.cargo/config.toml:4-6`,
`docs/adr/0015:9`, `crates/tablo-core/tests/it.rs:1-7`, and `ci.yml:22-25`
(which tells readers to run the `--precise` command
`CONTRIBUTING.md:99-121` forbids) each need a one-line correction in one
commit; re-measure the counts on the way past rather than trusting the
figures quoted here. Extend `pins_match_ci_and_docs`
(`xtask/src/gates/tests.rs:302`) from substrings to parsed vectors and
numbers so the gate checks revs, not just toolchains.

21. Two xtask guards disagree on locking. `xtask/src/lib.rs:137-140` runs
`cargo metadata` unlocked while `gates.rs:259-271` passes `--locked` with a
written rationale; a stale lockfile passes one guard (which heals it) and
fails the other.
Fix: add `--locked`; memoize the registry lookup in a `OnceLock`.
