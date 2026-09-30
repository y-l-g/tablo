# API quality sweep

Closes #TBD (open the proposal issue per `docs/dev/design/README.md` step 1,
then fill in the number).

Ordered findings, earliest first. Each item states the defect with a code
reference and the spec of the fix.

1. Upload bytes persist before the authoritative policy check.
`prepare_submission` in `panel/forms/submit.rs` stores uploads before the
advisory check runs and the transaction opens, with the authoritative
re-check inside the transaction on the edit path. A stale-advisory submit
writes bytes before the denial.
Fix: keep `store_uploads` outside the transaction as documented (external IO
must not run inside the held transaction, and rollback cannot undo it);
treat a stale-advisory denial as the same orphan class as validation-fail
orphans, owned by the app janitor. Optionally narrow the window by
re-checking the create policy inside the create transaction. Document the
upload step in the write flow in `docs/dev/architecture.md`, which never
mentions uploads.

2. `Field::relationship` accepts a query scope and discards it.
`schema/fields/mod.rs` takes a query closure and runs `let _ = query;`. Only
the argument value is inert, but an app that passes a scope believing it
filters gets every row.
Fix: delete the parameter; call sites drop the argument (showcase app, guide
forms page, relation and options tests):

```rust
Field::relationship::<AuthorResource>(|a| a.id.clone(), |a| a.name.clone())
```

3. The guide's only complete example cannot produce a working panel.
`docs/guide/src/first-panel.md` has no `#[layout]` calling
`Panel::layout_shell`, which is missing from the guide.
Fix: add the layout shell calling `Panel::layout_shell` to `first-panel.md`
(adapted from the showcase app, adding the imports a minimal example
needs), plus a copy-pasteable `[dependencies]` block carrying the real rev
pins and feature list, plus the stylesheet note (Tablo provides no
stylesheet, see `CONTEXT.md`).

4. The public API has no boundary and is unpublishable as configured. A large
`pub` surface, all crate-root-reachable, no `#[non_exhaustive]`, no sealed
module. `Table::render` has only test callers; panel serves
`render_with_state`. `panel::wired_table` is advertised as the third-party
seam in `docs/dev/architecture.md` but has no guide or showcase caller (only
a bench and a test). Publishing fails on versionless path deps in
`crates/tablo-core/Cargo.toml`, `include_str!` of `CONTEXT.md` escaping the
crate root, and missing `repository`/`homepage` metadata.
Fix: decide publish vs. git-only; if git-only, `publish = false` on all
crates. Either way: delete `Table::render` or route panel through it; land a
showcase page using `wired_table`, or make it `pub(crate)` and drop it from
the extension-point table. Then `#[non_exhaustive]` on `Mutation`,
`NavTarget`, `NotificationStatus`, `FieldErrorKind`, and opaque
`TableState`/`TablePage` behind accessors so a new URL param is not breaking.

5. The one-way layering claim is false, and the blast radius doc is off.
`resource/relation.rs` wires `crate::panel::relation_table`;
`resource/table/render/toolbar.rs` uses `crate::panel::table_search`; the
auth module imports panel while panel imports auth back. `schema/lenses.rs`
claims the `toasty_core` bridge is confined to a couple of modules, but more
production files name `toasty_core`.
Fix: a gate that fails on `crate::panel::` inside `resource/`, `schema/`,
and auth code (doc comments excluded); move `relation_table` into
`resource/table/render/`; move `parse_form_body` to a top-level module so
auth stops importing panel; rewrite the `lenses.rs` header to name the full
set.

6. `cargo xtask check` runs its gates serially, with traps. `xtask/src/gates.rs`
runs the gates serially. It hard-requires mdbook with no install hint and no
skip. `topcoat fmt` has no `--check`: it rewrites the tree, then fails on
`git diff --exit-code`, and the hint never says to revert.
Fix: tier it — `check` is test + clippy + fmt + `node --test`;
`check --full` is the dependency-shaped gates (MSRV, udeps, detached
benches, docs), triggered on manifest/lockfile/benchmark diffs. Classify
mdbook as optional with a skip message. Snapshot `git diff --name-only`
before `topcoat fmt` and print the revert command on failure. Single-source
the locked-rev install pipeline (today in `gates.rs`, `AGENTS.md`, and
`ci.yml`) behind `cargo xtask fmt --install`.

7. Three API taxes on the resource author. (a) Many `can_*` bodies in the
showcase are `true` or forward to `can_view_any`; no `Policy` value exists
(`CONTEXT.md` bans a Policy trait, nothing supplies the value).
Fix: keep default-deny and offer whole-resource allow/read-only/deny
shorthands as opt-in conveniences only, with per-record closures still
required for row-level rules:

```rust
fn policy() -> &'static Policy { Policy::allow_all() }
```

(b) `Table` has no composition; `Schema` does. `IntoColumns` accepts a value
or tuples; shared columns are re-spelled at each `Table::new`.
Fix: `impl<M> IntoColumns<M> for Vec<TextColumn<M>>` and for slices.
(c) `Resource::form` and `Resource::view` duplicate the same fields in the
showcase, and the view intentionally diverges (keys omitted, content and
relations rendered through dedicated seams).
Fix: keep the detail page opt-in on an explicitly declared view; offer a
read-only derivation helper for the subset of resources where showing the
form shape is appropriate, never as the default.

8. The write handlers are copies of one shape. The create/edit submit
handlers share gate, CSRF, `prepare_submission`, uniqueness, parse, invalid
re-render, and commit; the remaining delta (policy snapshot, record load,
completion, chrome, record function) stays per path. The single/bulk delete
handlers share gate, policy, parse, CSRF, and the confirm guard, then diverge
on single-key fetch plus single write vs id-list parse plus batched fetch
plus per-row checks plus batch write.
Fix: keep separate thin create/edit and single/bulk handlers and extract only
the shared guards and small helpers, leaving load, policy-check, and write
ordering explicit per path.

9. The benchmark apparatus is sized for a number nothing gates. The
benchmarks readme says the axum-maud and leptos comparators are compile-only
stubs; `verify_parity.sh` builds them but runs in no workflow.
`check_lockstep` compares only the upstream SHAs, which are in sync — while
registry drift between the two lockfiles is unguarded.
Fix: either gate the benchmark (commit a baseline, fail past an agreed
regression) or shrink it (delete the stubs, move tablo into the main
workspace).

10. The JS/Rust guard misses the riskiest hook. `verify_asset_hooks` covers
the Tablo-owned hooks; the JS also selects on a hook outside that list. The
one that matters is the swap-envelope template selector in
`mutation-submit.js`: Tablo never renders it and no Rust constant names it.
Some assets have no suite (`sidebar.js`, `theme.js`, `variant.js`), and
`docs/dev/TESTING.md` overclaims the coverage.
Fix: describe the swap-envelope selector as upstream-internal markup Tablo
reads but never renders and keep it out of the Tablo-owned hook registry;
assert in the showcase live-table check that a live-table body contains the
template; correct the `TESTING.md` coverage claim to name which assets lack
suites and what covers them. No browser harness: the JS assets stay
dependency-free with HTTP-level coverage.

11. `docs/dev/design/` contradicts the code and has no index.
`typed-record-form.md` specifies the generic `FieldErrors` signature; the
tree has non-generic `FieldErrors`. It also specifies
`FormResource`/`Panel::form_resource`, which the tree no longer contains
(`single-resource-registration.md` records the deletion).
Fix: delete `typed-record-form.md` (verify the other designs against the tree
on the way past), replace with an index table (design, issue, PR, owning
ADR). Keep `_template.md`.

12. `_Avoid_` lines have no checker, and one contradicts the code.
`CONTEXT.md` bans `Mutation` under the app-vocabulary heading, but
`Mutation` is a public enum re-exported at the crate root and used in the
guide. Several exported types (`RowKey`, `FieldLens`, `Sort`, `Cursor`,
`layout_shell`, `OptionSource`, `Authenticator`) appear in no guide page.
Fix: either delete the `_Avoid_` lines or table-drive them in a gate that
fails on a public identifier match. Add one "Extension points" guide page
covering `OptionSource`, `Authenticator`, `Uploader`, `Notification`,
`layout_shell`, and the `wired_table` recipe — it closes the doc gaps at
once.

13. The live shard binds the bulk signal without depending on it.
`resource/state.rs` says a checkbox click must not reload rows, while the
toolbar renders the bulk signal in the live branch; the list tests pin the
selection behavior.
Fix: add a query-count test first proving selection writes cause no row
reload; then align the shard comment with the bind-without-dependency
semantics and leave the transport where it is.

14. Relationship `<select>` order is driver-dependent. The base relationship
loader issues no `ORDER BY`; the `/options` endpoint applies
`R::order_by(cx)`.
Fix: one `if let` in the base loader.

15. `TabloError::Declaration` messages never reach a log. Both constructors
propagate as `Err` to the router rather than being dropped; the true defect
is narrower — the constructors never log, and the closed error type plus the
response mapper decide what anyone sees.
Fix: log in the constructors; test that the message reaches a tracing
subscriber. Leave response mapping to the existing error conversion.

16. The record-fn error doc says a record fn error is a server error — it is
passthrough. `panel/write.rs` keeps non-toasty mappings, so a
`create_record` returning `not_found()` answers with the app's own mapping
after the failure toast is already queued.
Fix the doc to passthrough semantics; leave any enveloping of app-authored
errors to an explicit opt-in, never the default path.

17. List renders rebuild the view `Schema` to answer a boolean. `R::viewed`
builds a full `Schema` to check emptiness, on every list page, every
live-search rerun, related tables, and once beside the real render on the
detail page. Measured: `view` is sync and IO-free by signature, and typical
views build a handful of fields, so each redundant call is pure construction
cost with no DB involvement; a separate boolean declaration would duplicate
view logic while losing request-conditional views.
Fix: keep the derivation and close as marginal unless profiling shows
otherwise.

18. Sequential relationship loads and bulk deletes. The shared node renderer
awaits each top-level node in turn; bulk delete fetches once and loops per
record through the `delete_record` default inside a single transaction, with
a capped id list as a DoS guard. Measured: concurrency is not a one-word
change (no combinator dependency, borrows forbid spawning, error semantics
would change), view-mode renders do no IO so only multi-relationship forms
would benefit, and the per-record delete loop is what preserves soft-delete
overrides with a single-statement batch already available as a documented
override.
Fix: keep sequential rendering until a `Send`-bound proof plus dependency
lands; document bulk delete as a capped fetch with per-record deletes
preserving overrides and a single-statement batch available via the
`bulk_delete_records` override.

19. Checked-in numbers contradict the tree. The cargo config, the test-binary
ADR, and the core test comment disagree with each other, and `ci.yml` tells
readers to run the `--precise` command `CONTRIBUTING.md` forbids. Re-measure
the numbers on the way past rather than trusting any figure quoted here.
Extend `pins_match_ci_and_docs` from substrings to parsed vectors and numbers
so the gate checks revs, not just toolchains.

20. Two xtask guards disagree on locking. `xtask/src/lib.rs` runs
`cargo metadata` unlocked while `gates.rs` passes `--locked` with a written
rationale; a stale lockfile passes one guard (which heals it) and fails the
other.
Fix: add `--locked`; memoize the registry lookup in a `OnceLock`.
