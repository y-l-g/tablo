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

2. The guide's only complete example cannot produce a working panel.
`docs/guide/src/first-panel.md` has no `#[layout]` calling
`Panel::layout_shell`, which is missing from the guide.
Fix: add the layout shell calling `Panel::layout_shell` to `first-panel.md`
(adapted from the showcase app, adding the imports a minimal example
needs), plus a copy-pasteable `[dependencies]` block carrying the real rev
pins and feature list, plus the stylesheet note (Tablo provides no
stylesheet, see `CONTEXT.md`).

3. The public API has no boundary and is unpublishable as configured. A large
`pub` surface, all crate-root-reachable, no sealed
module. `Table::render` has only test callers; panel serves
`render_with_state`. `panel::wired_table` is advertised as the third-party
seam in `docs/dev/architecture.md` but has no showcase caller (only
a bench and a test). Publishing fails on versionless path deps in
`crates/tablo-core/Cargo.toml`, and missing `repository`/`homepage` metadata.
Fix: decide publish vs. git-only; if git-only, `publish = false` on all
crates. Either way: delete `Table::render` or route panel through it; land a
showcase page using `wired_table`, or make it `pub(crate)` and drop it from
the extension-point table. Then `#[non_exhaustive]` on `NavTarget`,
`NotificationStatus`, `FieldErrorKind`, and opaque
`TableState`/`TablePage` behind accessors so a new URL param is not breaking.

4. The one-way layering claim is false, and the blast radius doc is off.
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

5. `cargo xtask check` runs its gates serially, with traps. `xtask/src/gates.rs`
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

6. Two API taxes on the resource author. (a) `Table` has no composition;
`Schema` does. `IntoColumns` accepts a value or tuples; shared columns are
re-spelled at each `Table::new`.
Fix: `impl<M> IntoColumns<M> for Vec<TextColumn<M>>` and for slices.
(b) `Resource::form` has a derived default, and the detail page is opt-in on
an explicitly declared view — but there is no read-only derivation helper
for the resources where showing the form shape is appropriate.
Fix: offer the helper for that subset, never as the default.

7. The write handlers are copies of one shape. Past the shared commit tail,
the create/edit submit handlers still copy gate, CSRF, `prepare_submission`,
uniqueness, parse, invalid re-render, policy snapshot, record load,
completion, chrome, and record function per path; the single/bulk delete
handlers share the confirm guard, then copy gate, policy, parse, CSRF, and
the fetch-and-write sequence around single-key vs id-list divergence.
Fix: keep separate thin create/edit and single/bulk handlers and extract the
shared guards, leaving load, policy-check, and write ordering explicit per
path.

8. The benchmark apparatus is sized for a number nothing gates. The
benchmarks readme says the axum-maud and leptos comparators are compile-only
stubs; `verify_parity.sh` builds them but runs in no workflow.
`check_lockstep` compares only the upstream SHAs, which are in sync — while
registry drift between the two lockfiles is unguarded.
Fix: either gate the benchmark (commit a baseline, fail past an agreed
regression) or shrink it (delete the stubs, move tablo into the main
workspace).

9. The JS/Rust guard misses the riskiest hook. `verify_asset_hooks` covers
the Tablo-owned hooks, and the swap-envelope template selector in
`mutation-submit.js` stays out of that registry as upstream-internal markup
Tablo reads but never renders. Some assets have no suite (`sidebar.js`,
`theme.js`, `variant.js`), and `docs/dev/TESTING.md` overclaims the
coverage.
Fix: assert in the showcase live-table check that a live-table body contains
the template; correct the `TESTING.md` coverage claim to name which assets
lack suites and what covers them. No browser harness: the JS assets stay
dependency-free with HTTP-level coverage.

10. `docs/dev/design/` contradicts the code and has no index.
`typed-record-form.md` specifies the generic `FieldErrors` signature; the
tree has non-generic `FieldErrors`. It also specifies
`FormResource`/`Panel::form_resource`, which the tree no longer contains
(`single-resource-registration.md` records the deletion).
Fix: delete `typed-record-form.md` (verify the other designs against the tree
on the way past), replace with an index table (design, issue, PR, owning
ADR). Keep `_template.md`.

11. `_Avoid_` lines have no checker, and one contradicts the code.
`CONTEXT.md` bans `Mutation` under the app-vocabulary heading, but
`Mutation` is a public enum re-exported at the crate root and used in the
guide. Several exported types (`RowKey`, `FieldLens`, `Sort`, `Cursor`,
`OptionSource`) appear in no guide page.
Fix: either delete the `_Avoid_` lines or table-drive them in a gate that
fails on a public identifier match. Add one "Extension points" guide page
covering the undocumented types — it closes the doc gaps at once.

12. The live shard binds the bulk signal without depending on it.
`resource/state.rs` says a checkbox click must not reload rows, while the
toolbar renders the bulk signal in the live branch; the list tests pin the
selection behavior.
Fix: add a query-count test first proving selection writes cause no row
reload; then align the shard comment with the bind-without-dependency
semantics and leave the transport where it is.

13. Relationship `<select>` order is driver-dependent. The base relationship
loader issues no `ORDER BY`; the `/options` endpoint applies
`R::order_by(cx)`.
Fix: one `if let` in the base loader.

14. `TabloError::Declaration` messages never reach a log. Both constructors
propagate as `Err` to the router rather than being dropped; the true defect
is narrower — the constructors never log, and the closed error type plus the
response mapper decide what anyone sees.
Fix: log in the constructors; test that the message reaches a tracing
subscriber. Leave response mapping to the existing error conversion.

15. The record-fn error doc says a record fn error is a server error — it is
passthrough. `panel/write.rs` keeps non-toasty mappings, so a
`create_record` returning `not_found()` answers with the app's own mapping
after the failure toast is already queued.
Fix the doc to passthrough semantics; leave any enveloping of app-authored
errors to an explicit opt-in, never the default path.

16. Checked-in numbers contradict the tree. The cargo config, the test-binary
ADR, and the core test comment disagree with each other, and `ci.yml` tells
readers to run the `--precise` command `CONTRIBUTING.md` forbids. Re-measure
the numbers on the way past rather than trusting any figure quoted here.
Extend `pins_match_ci_and_docs` from substrings to parsed vectors and numbers
so the gate checks revs, not just toolchains.

17. Two xtask guards disagree on locking. `xtask/src/lib.rs` runs
`cargo metadata` unlocked while `gates.rs` passes `--locked` with a written
rationale; a stale lockfile passes one guard (which heals it) and fails the
other.
Fix: add `--locked`; memoize the registry lookup in a `OnceLock`.
