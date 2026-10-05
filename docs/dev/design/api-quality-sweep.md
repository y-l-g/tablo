# API quality sweep

Closes #TBD (open the proposal issue per `docs/dev/design/README.md` step 1,
then fill in the number).

Ordered findings, earliest first. Each item states the defect with a code
reference and the spec of the fix.

1. Upload bytes persist before the authoritative policy check.
`prepare_submission` in `panel/forms/submit.rs` stores uploads after the
advisory check and before the transaction opens, with the authoritative
re-check inside the transaction on the edit path. A stale-advisory submit
writes bytes before the denial; validation failures strand bytes the same
way.
Fix: treat a stale-advisory denial as the same orphan class as
validation-fail orphans, owned by the app janitor. Document the upload step
in the write flow in `docs/dev/architecture.md`, which never mentions
uploads.

2. The public API has no boundary and is unpublishable as configured. A large
`pub` surface, all crate-root-reachable, no sealed module.
`panel::wired_table` is advertised as the third-party seam in the guide and
rustdoc; it has a bench, a test, and a guide example, but no showcase
caller. Publishing fails on versionless intra-workspace path deps in
`crates/tablo-core/Cargo.toml`, and missing `repository`/`homepage` metadata.
Fix: decide publish vs. git-only; if git-only, `publish = false` on all
crates. Either way: land a showcase page using `wired_table`, or make it
`pub(crate)` and drop it from the extension-point table. Then
`#[non_exhaustive]` on `NavTarget`, `NotificationStatus`, `FieldErrorKind`,
and opaque `TableState`/`TablePage` behind accessors so a new URL param is
not breaking.

3. `cargo xtask check` runs its gates serially, with traps. `xtask/src/gates.rs`
runs the gates serially. It hard-requires mdbook with no install hint and no
skip. `topcoat fmt` has no `--check`: it rewrites the tree, then fails on
`git diff --exit-code`, and the hint never says to revert.
Fix: tier it — `check` is test + clippy + fmt + `node --test`;
`check --all` is the dependency-shaped gates (MSRV, udeps, detached
benches, docs), triggered on manifest/lockfile/benchmark diffs. Classify
mdbook as optional with a skip message. Snapshot `git diff --name-only`
before `topcoat fmt` and print the revert command on failure. Single-source
the pinned-CLI install pipeline (today in `gates.rs`, `AGENTS.md`, and
`ci.yml`) behind `cargo xtask fmt --install`.

4. `Table` has no composition; `Schema` does. `IntoColumns` accepts a value
or tuples; shared columns are re-spelled at each `Table::new`.
Fix: `impl<M> IntoColumns<M> for Vec<BoxColumn<M>>` and for slices.

5. The benchmark apparatus is sized for a number nothing gates. The
benchmarks readme says the axum-maud and leptos comparators are compile-only
stubs; `verify_parity.sh` builds them but runs in no workflow; the workspace
and bench lockfiles can drift with no version-parity gate.
Fix: gate version parity between the workspace and bench lockfiles.

6. The JS/Rust guard misses the riskiest hook. `verify_asset_hooks` covers
the Tablo-owned hooks, and the swap-envelope template selector in
`mutation-submit.js` stays out of that registry as upstream-internal markup
Tablo reads but never renders. Some assets have no suite (`sidebar.js`,
`theme.js`, `variant.js`), and `docs/dev/TESTING.md` does not name which
assets lack suites or what covers them.
Fix: assert in the showcase live-table check that a live-table body contains
the template; correct the `TESTING.md` coverage claim to name which assets
lack suites and what covers them. No browser harness: the JS assets stay
dependency-free with HTTP-level coverage.

7. `_Avoid_` lines have no checker, and one contradicts the code.
`CONTEXT.md` bans `Mutation` under the app-vocabulary heading, but
`Mutation` is a public enum re-exported at the crate root and used in the
guide. Several exported types (`Sort`, `Cursor`, `OptionSource`) appear in
no guide page.
Fix: either delete the `_Avoid_` lines or table-drive them in a gate that
fails on a public identifier match. Add one "Extension points" guide page
covering the undocumented types — it closes the doc gaps at once.

8. The live shard threads the bulk signal through without loading from it.
`panel/search.rs` passes `bulk` through the search shard while `list_search`
derives state from `query` only, and the live toolbar branch in
`table/render/toolbar.rs` binds the bulk signal. No query-count test covers
selection writes.
Fix: add a query-count test proving selection writes cause no row reload;
leave the transport where it is.

9. `TabloError::Declaration` messages never reach a log. Both constructors
propagate as `Err` to the router rather than being dropped; the true defect
is narrower — the constructors never log, and the closed error type plus the
response mapper decide what anyone sees.
Fix: log in the constructors; test that the message reaches a tracing
subscriber. Leave response mapping to the existing error conversion.

10. The pins gate does not cover the pinned tool versions.
`pins_match_ci_and_docs` checks substrings only and covers neither the
pinned `topcoat-cli` version in `xtask/src/gates.rs` nor the pinned mdbook
version in `ci.yml`. Re-measure the pins on the way past rather than trusting
any figure quoted here.
Fix: extend `pins_match_ci_and_docs` to parsed versions so the gate checks
tool versions, not just toolchains.
