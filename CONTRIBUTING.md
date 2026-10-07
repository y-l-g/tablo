# Contributing to Tablo

Small fixes, documentation corrections, and tests can go straight to a pull request. For a new
feature or a public-API change, open an issue first and describe the problem: the [feature
proposal form](.github/ISSUE_TEMPLATE/feature_proposal.yml) asks for the problem, an API sketch,
the alternatives and a scope estimate, so the issue is where the design is discussed. Read
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
showcase`); the JavaScript suites run with `node --test`, and gate 6 below names them.

## The gate set

CI runs seven gates plus three extra checks (mirroring `.github/workflows/ci.yml` and, for
gates 5 and 7, `.github/workflows/msrv-udeps.yml`).
The fast path is the xtask runner: `check` runs the cheap gates below fail-fast, cheapest
first, skipping the ones CI would not run for the change; `check --all` runs every gate,
including the slow docs and external builds.

```sh
cargo xtask check         # the cheap gates below, skipping CI-skipped ones
cargo xtask check --all   # every gate, plus docs and the external build
cargo xtask fmt           # the formatting subset: nightly fmt, detached fmt, pinned topcoat fmt
```

`check` runs each command below in execution order, stopping at the first failure: gates 3,
4, 6, then the detached fmt, then gates 1, 2, 5, 7. Gates 5 and 7 run only when the change
touches the `msrv-udeps.yml` path filter; any git failure runs every gate.
`check --all` runs every gate below plus the extras.

1. `cargo test --workspace --locked`
2. `cargo clippy --workspace --all-targets --locked -- -D warnings`
3. `cargo +nightly-2026-08-24 fmt --all -- --check`
4. `topcoat fmt`, then `git diff --exit-code`
5. `cargo +1.98 check --workspace --locked`
6. `node --test crates/tablo-ui/assets/selects.test.js examples/showcase/assets/media.test.js`
7. `cargo +nightly install cargo-udeps --locked`, then `cargo +nightly udeps --workspace --all-targets --locked`

Gate 3 runs on the dated nightly in `rust-toolchain.toml`: `rustfmt.toml`'s keys are
nightly-only (GH #269). Gate 5 is the MSRV floor in `Cargo.toml` (GH #175).
Gate 7 guards unused dependencies (GH #271).

CI runs three more checks outside the seven, and a change touching what they cover
has to pass them too (`cargo xtask check --all` runs the detached fmt with the cheap gates
up front, then docs and the external build after gate 7):

- the `docs` job builds rustdoc with
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`, then
  builds the guide with `mdbook build docs/guide`;
- the `fmt` job runs `cargo fmt -- --check` inside the detached
  `examples/quickstart` workspace;
- the `external` job runs `cargo xtask external-check`: the detached
  `examples/quickstart` app must build, serve, and generate a stylesheet with
  classes only Tablo's own sources write, so workspace-only resolutions fail it.

### The `topcoat fmt` trap

The `topcoat` CLI on `PATH` is usually not the release this workspace uses,
and `topcoat fmt` reflows `view!` markup differently across releases. CI
installs `topcoat-cli 0.10.0` before formatting, so a locally
installed CLI of another version proposes a diff CI rejects. Do not hand-fix
that diff. `cargo xtask fmt` runs the check half only: it never installs the
CLI, and a missing or wrong-version CLI fails with the pinned install command.
Install `topcoat-cli 0.10.0` and run it — the exact command is
the `Install topcoat CLI` step of the `fmt` job in
[`.github/workflows/ci.yml`](.github/workflows/ci.yml).

## Vendored primitives

`crates/tablo-ui/src/components/primitives/` mirrors the `topcoat-ui-registry`
crate verbatim, under a `SYNC` header recording the registry version and the
source hash. Never hand-edit those files: update them with
`cargo xtask sync-topcoat-ui`. `cargo xtask verify-topcoat-ui` fails when a
vendored file has drifted. Components Tablo owns live in
`crates/tablo-ui/src/components/composites/` and are edited normally
(ADR-0007).

## Dependency pins

`topcoat` and `toasty` are crates.io dependencies in two manifests: the workspace's and
`examples/quickstart`'s. Renovate groups their bumps; a coupled set, such as `argon2` with
`password-hash`, merges as one combined manual bump. A bump touches both manifests and the
workspace lockfile (the quickstart commits none) in one commit:

```sh
cargo update -p topcoat -p toasty
```

Never run a blanket `cargo update`. Two `syn` majors remain (GH #181, GH #193); do not
force-unify them. To work against a local upstream checkout, add an uncommitted `[patch]`
section pointing at `../topcoat` or `../toasty`.

## Commits

Every branch is squash-merged into `master`: one commit per branch, so no empty merge commits,
and a branch's own commits are working notes. The squashed commit is a Conventional Commit:

```
<type>(<scope>): <description> (#123)
```

`type` is one of `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `perf`, `chore`, `build`,
`ci`, `revert`. `scope` names the subsystem the change touches: `table`, `panel`, `schema`,
`core`, `ui`, `xtask`, `showcase`, `docs`, `repo`. The subject is imperative and lowercase, has
no trailing period, and ends with the issue reference when the change closes an issue. No line
of the message is longer than 100 characters. A breaking change carries `!` after the type or
scope and a `BREAKING CHANGE:` footer explaining it. The body says what changed and why without
restating the diff.

Pull request titles follow the same format, since the title becomes the merged commit;
reviewers check it.

## Triage

Maintainers close issues and pull requests without detailed review when a change
does not align with the project's direction, duplicates existing work, or is not
worth the time to review. If context changes the picture, follow up in the thread.

## Decisions and vocabulary

Record durable design decisions in [`docs/adr/`](docs/adr/): one record per decision, keeping
only the decision, the rejected alternatives, and the constraint future code must respect.
Behaviour lives in the guide and in rustdoc. Domain terms and the synonyms to avoid live in
[`CONTEXT.md`](CONTEXT.md); use its words in code, issues, and commits. The rules for prose,
tests, commits and issues are in [`AGENTS.md`](AGENTS.md).

By contributing, you agree that your contributions are licensed under the
[MIT license](LICENSE).
