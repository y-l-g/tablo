# Contributing to Tablo

Small fixes, documentation corrections and tests can go straight to a pull request. A new feature
or a public-API change starts with a feature-proposal issue; one that reshapes `Panel`,
`Resource`, `Table`, `Schema` or the policy and tenancy seams waits for the issue's API sketch to
be accepted before the implementation PR.

## Layout

| Path | Contents |
| --- | --- |
| `crates/tablo` | the facade an app depends on: `tablo-core` at its root, `ui`, `testing`, `prelude`, the driver features |
| `crates/tablo-core` | Panel, Resource, Table, Schema, policy, auth, tenancy, uploads |
| `crates/tablo-macros` | the `RecordForm`, `EmbeddedForm`, `ActionInput` and `Options` derives |
| `crates/tablo-ui` | `primitives/` synced from `topcoat-ui-registry`, `composites/` owned here |
| `crates/tablo-test` | the in-memory HTTP client, re-exported as `tablo::testing` |
| `crates/tablo-build` | `tailwind()`, the app's Tailwind build over Tablo's sources |
| `examples/showcase` | the runnable admin and the integration suite |
| `examples/guide` | the guide's compiled snippets, included by `docs/guide/` |
| `examples/quickstart` | the smallest app, outside the workspace, built from outside the repo by `external-check` |
| `benchmarks/` | the list-path benchmark |
| `xtask` | the check runner, the primitives sync, and the repo guards |

No library crate enables a database driver; the app picks one through a `tablo` feature.

```sh
cargo run -p showcase   # http://localhost:3000/admin/users
```

## Checks

```sh
cargo xtask check         # fmt, JS suites, test, clippy; MSRV and udeps if manifests changed
cargo xtask check --all   # also MSRV, udeps, rustdoc, the guide and the external build
cargo xtask fmt           # the formatting checks alone
```

`check` stops at the first failure, cheapest first. "Manifests changed" means any `Cargo.toml`,
`Cargo.lock`, `rust-toolchain.toml` or `msrv-udeps.yml` differs from `master`. CI runs the same commands in
`.github/workflows/ci.yml` and `msrv-udeps.yml`, which hold the pinned toolchains. Run
`check --all` before merging.

- `topcoat fmt` reflows `view!` markup differently across CLI releases, so only the pinned
  `topcoat-cli` agrees with CI. When it is missing or another version, `cargo xtask fmt` fails
  with the install command. Never hand-fix its diff.
- `examples/quickstart` is outside the workspace: `cargo fmt` at the root skips it, and
  `cargo xtask fmt` checks it.
- `crates/tablo-ui/src/components/primitives/` is a verbatim copy of the registry. Never edit it:
  `cargo xtask sync-topcoat-ui` updates it and `cargo test -p xtask` fails on drift.

## Code

- A struct is followed by its inherent `impl`, then its trait impls. A module is `foo.rs`
  beside `foo/`, never `foo/mod.rs`.
- Shared dependency versions live in the root `[workspace.dependencies]`.
- `topcoat` and `toasty` are crates.io dependencies of the workspace and of
  `examples/quickstart`. Bump them in both manifests and the lockfile together with
  `cargo update -p topcoat -p toasty`; never run a blanket `cargo update`. To edit against a
  local checkout, add an uncommitted `[patch]` pointing at `../topcoat` or `../toasty`.
- Read an upstream API from the version `Cargo.lock` pins, in the cargo registry cache, not from
  memory or the upstream branch tip.
- Use the words `CONTEXT.md` defines, in code, issues and commits.

## Tests

- A test pins a behavior that a plausible bug breaks. Derive the expected value from the intended
  behavior, never by re-running the implementation.
- Assert structure and state (row counts, redirects, link targets, database rows), not wording,
  through the `tablo-test` queries (`rows`, `row_actions`, `field_error`, `filter_options`).
- A behavior is pinned once, by a unit test or an integration test. Delete a test that proves
  nothing.
- Unit tests live in `tests.rs` beside their source file (`foo/tests.rs` for `foo.rs`).
- `examples/showcase/tests/it.rs` is one test binary whose modules are the suite files:
  `cargo test -p showcase --test it admin::` runs one file. `tests/framework/` covers what the
  showcase's resources do not reach, with test-local models.

## Writing

Behavior is documented in rustdoc and the guide (`docs/guide/`), vocabulary in `CONTEXT.md`,
and decisions in `docs/adr/`. `README.md` is the entry point.

- Verify every claim against the code before writing it.
- Document current behavior only: no history, no plans. History belongs in a commit message.
- Active voice, present tense. Show the call or the output rather than describing it.
- No filler, hype, weasel words or metaphors: say "by default", not "out of the box"; say what the
  code does, not "under the hood" or "magic".
- A comment explains why: an invariant, an upstream workaround, a safety argument. It never
  restates the next line.
- An ADR records one decision and the alternatives it rejected, and is amended in place. A retired
  number is never reused.

## Pull requests and commits

Every branch is squash-merged into `master`, so the PR title becomes the commit: a Conventional
Commit under 100 characters, ending with the issue it closes, if any.

```
fix(table): bound the filters signal (#205)
```

- Types: `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `ci`, `build`, `chore`, `revert`.
- Scopes: `core`, `table`, `schema`, `panel`, `auth`, `ui`, `macros`, `test`, `build`, `guide`,
  `showcase`, `bench`, `xtask`, `deps`, `release`, `repo`.
- A breaking change adds `!` after the scope and a `BREAKING CHANGE:` footer.
- The body says what changed and why. The PR description adds which checks ran.

## Issues

File through a form in `.github/ISSUE_TEMPLATE/`; it sets the label, and maintainers set the rest
from `.github/labels.yml`. An upstream gap is one missing Toasty or Topcoat API per issue, and its
body is its status: edit the body, do not comment.

By contributing, you agree that your contributions are licensed under the [MIT license](LICENSE).
