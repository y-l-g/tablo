---
name: check
description: Always use this skill to verify a change locally before committing or opening a pull request in the Tablo repository
---

# Verifying a Change

Run `cargo xtask check`: the gates in
[`CONTRIBUTING.md`](../../../CONTRIBUTING.md#the-gate-set), run in order with a
stop at the first failure. `--quick` leaves out the gates that need another
toolchain or a build of a detached workspace. To run one gate by hand, that list
is the canonical copy (it mirrors `.github/workflows/ci.yml` and, for gates 6 and
8, `.github/workflows/msrv-udeps.yml`); the extra checks outside the eight (docs,
detached-bench fmt, external, bench-check) are listed there too. `cargo xtask fmt`
covers the formatting subset alone.

The asset suites are named rather than globbed, exactly as the CI `assets` job
names them: a glob would silently shrink the run when a suite is renamed, while
a missing path fails the job. Gate 3's nightly date is recorded in
`rust-toolchain.toml`'s comment, so the nightly-only rustfmt keys `rustfmt.toml`
sets cannot move under the gate (GH #269).

Rules that catch the recurring failures:

- `topcoat fmt` only agrees with the CLI built from the rev `Cargo.lock` pins.
  Another CLI's diff is not a fix: install the locked rev (see
  [`CONTRIBUTING.md`](../../../CONTRIBUTING.md#the-topcoat-fmt-trap)) and run that.
- `cargo fmt` covers workspace members only; the detached `benchmarks/*`
  workspaces are formatted and linted by manifest path.
- Any lockfile change syncs `benchmarks/tablo/Cargo.lock` in the same commit,
  with identical `topcoat`/`toasty` revs: bump with
  `cargo xtask bump-upstream <TOPCOAT_REV> <TOASTY_REV>`, prove with
  `cargo xtask verify-locks`.
- Give each worktree its own target directory; a shared `CARGO_TARGET_DIR`
  cross-contaminates.
- Never pipe when you need the exit code: `| tail` masks it. Read `PIPESTATUS`
  or redirect to a file.
- Never hand-edit `crates/tablo-ui/src/components/primitives/`; sync it with
  `cargo xtask sync-topcoat-ui`.
- `cargo udeps` needs `cargo-udeps` on nightly for `-Z binary-dep-depinfo`:
  `cargo +nightly-2026-08-24 install cargo-udeps --version 0.1.61 --locked`, then
  the udeps gate in
  [`CONTRIBUTING.md`](../../../CONTRIBUTING.md#the-gate-set). The gate probes
  that pinned version and skips the install when it already answers.
- A gate whose command names a toolchain installs it on demand; gate 3's dated
  nightly install is the `rustup toolchain install` step of the `fmt` job in
  `.github/workflows/ci.yml`, and gate 8's is the same step in
  `.github/workflows/msrv-udeps.yml`.
