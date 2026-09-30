# Contributing to Tablo

Small fixes, documentation corrections, and tests can go straight to a pull request. For a new
feature or a public-API change, open an issue first and describe the problem: redirecting a
design is cheaper than redirecting a patch. A change that reshapes `Panel`, `Resource`, `Table`,
`Schema`, or the policy/tenancy seams is a good candidate for a design document under
[`docs/dev/design/`](docs/dev/design/) first, on a trial basis: open the design, land it
without implementation, then implement once it is accepted. Read
[`AGENTS.md`](AGENTS.md) before your first change; it holds the rules this document expands.

## Fork and branch

Fork the repository and branch off `master`. Keep the branch mergeable by rebasing onto `master`
rather than merging `master` into it. Branches squash-merge, so no history tidying is needed
before pushing. Keep pull requests focused: if a fix grows into a feature or a redesign, discuss
the scope before continuing.

## Using AI assistants

AI-assisted contributions are welcome, with no disclosure required.

## Build and run

```sh
cargo run -p showcase
# open http://localhost:3000/admin/users
```

`crates/tablo-core` is the framework. `examples/showcase` is the runnable admin, the reference
for panel and resource declarations, and the home of the integration tests (`cargo test -p
showcase`); the JavaScript unit tests are `node --test crates/tablo-ui/assets/*.test.js`
(the explicit suite list is gate 7 in the gate set below).

## The gate set

CI runs eight gates plus four extra checks (mirroring `.github/workflows/ci.yml`;
this list is the canonical copy — `AGENTS.md` and the `check` skill point here).
The fast path is the xtask runner: gates are mutually independent, and `check`
runs each command below in order, stopping at the first failure.

```sh
cargo xtask check   # the eight gates plus the extras
cargo xtask fmt     # the formatting subset: nightly fmt, detached-bench fmt, locked-rev topcoat fmt
```

The raw commands — the expansion of `cargo xtask check`:

1. `cargo test --workspace --locked`
2. `cargo clippy --workspace --all-targets --locked -- -D warnings`
3. `cargo +nightly-2026-08-24 fmt --all -- --check`
4. `topcoat fmt`, then `git diff --exit-code`
5. `cargo clippy --locked --manifest-path benchmarks/tablo/Cargo.toml --all-targets -- -D warnings`
6. `cargo +1.98 check --workspace --locked`
7. `node --test crates/tablo-ui/assets/selects.test.js crates/tablo-ui/assets/bulk.test.js crates/tablo-ui/assets/wire.test.js crates/tablo-ui/assets/dialog.test.js crates/tablo-ui/assets/mutation-submit.test.js crates/tablo-ui/assets/notifications.test.js crates/tablo-ui/assets/filters.test.js crates/tablo-ui/assets/live-search.test.js examples/showcase/assets/media.test.js`
8. `cargo +nightly install cargo-udeps --locked`, then `cargo +nightly udeps --workspace --all-targets --locked`

Gate 3 runs on the dated nightly in `rust-toolchain.toml`: `rustfmt.toml`'s keys are
nightly-only (GH #269). Gate 6 is the MSRV floor in `Cargo.toml` (GH #175).
Gate 8 guards unused dependencies (GH #271). Rustup installs a missing toolchain on first use.

CI runs four more checks outside the eight, and a change touching what they cover
has to pass them too (`cargo xtask check` runs all four after the eight):

- the `docs` job builds rustdoc with
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`, then
  builds the guide with `mdbook build docs/guide`;
- the `fmt` job runs `cargo fmt -- --check` inside each detached `benchmarks/*`
  workspace (`benchmarks/tablo`, `benchmarks/axum-maud`, `benchmarks/leptos`) —
  part of `cargo xtask fmt`;
- the `bench-check` job compiles the detached harness (gate 5 above) and
  verifies that `Cargo.lock` and `benchmarks/tablo/Cargo.lock` pin identical
  `topcoat` and `toasty` revs and that both manifests' `rev =` pins agree
  (`cargo xtask verify-locks`, also run by the
  xtask test suite on every `cargo test`).

### The `topcoat fmt` trap

The `topcoat` CLI on `PATH` is usually not the revision this workspace locks,
and `topcoat fmt` reflows `view!` markup differently across revisions. CI
installs the CLI at the locked revision before formatting, so a locally
installed CLI of another version proposes a diff CI rejects. Do not hand-fix
that diff. `cargo xtask fmt` runs the check half only: it never installs the
CLI, and a missing or wrong-rev CLI fails with the locked-rev install command.
Install the CLI at the locked rev and run it — the exact command is
the `Install topcoat CLI` step of the `fmt` job in
[`.github/workflows/ci.yml`](.github/workflows/ci.yml).

## Vendored primitives

`crates/tablo-ui/src/components/primitives/` mirrors the `topcoat-ui-registry`
crate verbatim, under a `SYNC` header recording the registry version and the
source hash. Never hand-edit those files: update them with
`cargo xtask sync-topcoat-ui`. `cargo xtask verify-topcoat-ui` fails when a
vendored file has drifted, and the xtask test suite runs it on every
`cargo test`. Components Tablo owns live in
`crates/tablo-ui/src/components/composites/` and are edited normally
(ADR-0007).

## Dependency pins

`topcoat` and `toasty` are git dependencies pinned to exact `rev`s in both
manifests. Never run a blanket `cargo update`. Bump them deliberately with one
command:

```sh
cargo xtask bump-upstream <TOPCOAT_REV> <TOASTY_REV>
```

It rewrites the `rev =` pins for both upstream repos in both manifests
(`toasty-core` and `topcoat-ui*` track their repo's rev), re-resolves both
lockfiles, proves the new revs resolve from the local git cache
(`cargo check --offline`), and asserts lockstep. The expansion:

```sh
# new revs into Cargo.toml and benchmarks/tablo/Cargo.toml, then:
cargo update -p topcoat -p toasty
cargo update --manifest-path benchmarks/tablo/Cargo.toml -p topcoat -p toasty
cargo check --offline
cargo check --offline --manifest-path benchmarks/tablo/Cargo.toml
cargo xtask verify-locks
```

Drift means the benchmark measures different upstream code than the workspace
builds.

## Commits

Every branch is squash-merged into `master`: one commit per branch, so no empty
merge commits. The squashed commit is a Conventional Commit with the issue
reference in the subject. [`docs/dev/COMMITS.md`](docs/dev/COMMITS.md) is the
authoritative format. Pull request titles follow the same format, since the
title becomes the landed commit; reviewers check it.

## Triage

Maintainers close issues and pull requests without detailed review when a change
does not align with the project's direction, duplicates existing work, or is not
worth the time to review. Closures are routine and carry no judgment: if context
changes the picture, follow up in the thread.

## Decisions and vocabulary

Record durable design decisions in [`docs/adr/`](docs/adr/). Domain terms and
the synonyms to avoid live in [`CONTEXT.md`](CONTEXT.md); use its words in code,
issues, and commits. All human-readable text follows
[`docs/dev/PROSE.md`](docs/dev/PROSE.md). Test discipline lives in
[`docs/dev/TESTING.md`](docs/dev/TESTING.md).

By contributing, you agree that your contributions are licensed under the
[MIT license](LICENSE).
