# Tablo — Agent Instructions

## Commands

The gate set lives in [`CONTRIBUTING.md`](CONTRIBUTING.md#the-gate-set): seven commands
mirroring `.github/workflows/ci.yml` and `.github/workflows/msrv-udeps.yml`. Run it via `cargo xtask check` (fail-fast,
cheapest-first, skipping the MSRV/udeps gates CI skips for the change),
or the ones covering your change; `cargo xtask check --all` before merging. CI also runs the extra
checks listed there (docs, detached fmt, guide, external).

```sh
cargo xtask check                                # the gate set, fail-fast
cargo xtask fmt                                  # nightly fmt + detached fmt + pinned topcoat fmt
cargo xtask external-check                       # build and test examples/quickstart outside the repo
cargo run -p showcase                            # http://localhost:3000/admin/users
cargo xtask sync-topcoat-ui                      # re-vendor primitives, verbatim
cargo xtask verify-topcoat-ui                    # fail on vendored drift

# `topcoat fmt` only agrees with the pinned CLI version.
cargo install topcoat-cli --version 0.10.0 --locked --force
```

## Rules

1. Verify every factual claim in a doc, comment, or commit message against the code.
2. Document current behavior only; no "used to", "previously". See `CONTRIBUTING.md#prose`.
3. Run the gate set for the area you touched, plus `cargo test --workspace --locked` on the
   merged result: branches can merge cleanly and not compile.
4. Give each worktree its own target directory; a shared `CARGO_TARGET_DIR` cross-contaminates.
5. Never pipe when you need the exit code: `| tail` masks it. Read `PIPESTATUS` or redirect to
   a file.
6. `cargo fmt` covers workspace members only; the detached `examples/quickstart` and
   `examples/guide` packages are formatted and linted by manifest path.
7. The release PR pins `examples/guide`'s tablo versions and lockfile to the newest release tag.
8. Never hand-edit `crates/tablo-ui/src/components/primitives/`; sync it with xtask. Owned
   components live in `components/composites/`.
9. Hunting dead code: prefer `pub` API, always-same-value config, and test-only paths.
   `unsafe_code` and `warnings` are denied; `too_many_lines` is denied with the
   budget in the workspace-root `clippy.toml` (`too-many-lines-threshold = 300`).
10. Run `topcoat fmt` with the pinned CLI after changing `view!` markup; another CLI's
    diff is not a fix. See `CONTRIBUTING.md`.

## Git

Squash-merge every branch into `master` — one commit per branch, no empty merge commits; a
branch's commits are working notes. The squashed commit is a Conventional Commit, carrying the
issue in the subject when the change closes one (`docs/dev/COMMITS.md`).

## Layout

Crate roles live in [`docs/dev/architecture.md`](docs/dev/architecture.md#crates). The user
guide is `docs/guide/` (mdBook), decisions are in `docs/adr/`, contributor specs in
`docs/dev/`, domain vocabulary in `CONTEXT.md`, and agent tracker notes in `docs/agents/`.

## Renovate PRs

Renovate groups `topcoat`/`toasty` bumps. Coupled sets (e.g. `argon2` +
`password-hash`) merge as one combined manual bump.
Two `syn` majors remain (GH #181, GH #193); do not force-unify.

## Further reading

- Build and verify: [`CONTRIBUTING.md`](CONTRIBUTING.md), `docs/dev/architecture.md`,
  `docs/dev/TESTING.md`
- Write: [`CONTRIBUTING.md`](CONTRIBUTING.md#prose), `docs/dev/COMMITS.md`, `docs/dev/LABELS.md`,
  `docs/guide/`, `CONTEXT.md`
- Decide: `docs/adr/`, `docs/dev/design/`, `docs/dev/upstream-notes.md`, `docs/agents/`.
