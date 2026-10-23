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
| `crates/tablo-macros` | the `ModelColumns`, `RecordForm`, `ModelForm`, `EmbeddedForm`, `FormInput` and `Options` derives |
| `crates/tablo-ui` | `primitives/` synced from `topcoat-ui-registry`, `composites/` owned here |
| `crates/tablo-test` | the in-memory HTTP client, re-exported as `tablo::testing` |
| `crates/tablo-build` | `tailwind()`, the app's Tailwind build over Tablo's sources |
| `examples/showcase` | the runnable admin and the suite over its resources |
| `examples/guide` | the guide's compiled snippets, included by `docs/guide/` |
| `integration` | the integration suite over test-local models, unpublished |
| `examples/quickstart` | the smallest app, outside the workspace, built from outside the repo by `external-check` |
| `benchmarks/` | the list-path benchmark |
| `xtask` | the check runner, the primitives sync, and the repo guards |

No library crate enables a database driver; the app picks one through a `tablo` feature.

```sh
topcoat dev -p showcase   # http://localhost:3000/admin/users
```

`topcoat dev` builds the showcase and the asset bundle it needs; `cargo run -p showcase` stops
without one. The README has the install command.

## Checks

```sh
cargo xtask check         # fmt, JS suites, test, clippy; udeps if relevant
cargo xtask check --all   # also udeps, rustdoc, the guide and the external build
cargo xtask fmt           # the formatting checks alone
```

`check` stops at the first failure, cheapest first. udeps runs when any `Cargo.toml`,
`Cargo.lock`, `rust-toolchain.toml` or `udeps.yml` differs from `master`. CI runs
the same commands in `.github/workflows/ci.yml` and `udeps.yml`, which hold the pinned
toolchains. Run `check --all` before merging.

`rust-toolchain.toml` pins the toolchain to the workspace `rust-version`, so every build runs the
MSRV. Bump both in one PR.

- `topcoat fmt` reflows `view!` markup differently across CLI releases, so only the pinned
  `topcoat-cli` agrees with CI. When it is missing or another version, `cargo xtask fmt` fails
  with the install command. Never hand-fix its diff.
- `examples/quickstart` is outside the workspace: `cargo fmt` at the root skips it, and
  `cargo xtask fmt` checks it.
- `crates/tablo-ui/src/components/primitives/` is a verbatim copy of the registry. Never edit it:
  `cargo xtask sync-topcoat-ui` updates it and `cargo test -p xtask` fails on drift.

## Code

- A struct is followed by its inherent `impl`, then its trait impls. A module is `foo.rs`
  beside `foo/`, never `foo/mod.rs`. The one exception is `tests/common/mod.rs`, which Cargo would
  otherwise build as a test binary of its own.
- A source holds at most 600 lines, a test source 1000. `xtask/src/sizes.rs` lists the files
  past that, each capped at its current size; split one rather than raise its cap, lower the cap
  when the file shrinks, and drop it from the list once it fits.
- Code generic over a model or resource is compiled again in every app crate, once per type.
  Keep what does not read the type out of it: a view built from strings goes in a free function,
  not a method of `impl<M>`, and a future only a generic caller awaits is returned boxed, as
  `BoxFuture`, so tablo-core compiles it once.
- A public item has one path. Types sit at the `tablo-core` root, re-exported from private
  modules. A public module is a namespace for functions and constants that read best qualified
  (`auth::user`, `csrf::verify`, `url::edit`), plus the types only that namespace uses, and it
  never re-exports a root item.
- An `expect` names the invariant that makes its failure unreachable. Anything a request,
  a database row or an app's declaration can cause is an error, never a panic.
- Shared dependency versions live in the root `[workspace.dependencies]`.
- `topcoat` and `toasty` are crates.io dependencies of the workspace and of
  `examples/quickstart`. Bump them in both manifests and the lockfile together with
  `cargo update -p topcoat -p toasty`; never run a blanket `cargo update`. To edit against a
  local checkout, add an uncommitted `[patch]` pointing at `../topcoat` or `../toasty`.
- A committed `[patch]` to an upstream git revision is a stopgap until that upstream releases.
  CI's `no dependency patch` job fails a release PR while one stands, since `cargo publish`
  ignores it. A project depending on Tablo by path or git needs the same `[patch]`.
- Read an upstream API from the version `Cargo.lock` pins, in the cargo registry cache, not from
  memory or the upstream branch tip.
- Use the words `CONTEXT.md` defines, in code, issues and commits.
- A word a page or a mail shows is a message of `crates/tablo-core/locales/en.ftl`, its id
  starting `tablo-`, and each other catalog there translates it with the same variables; a label
  a builder takes is a `Text`. A browser script reads its words from a `data-` attribute the
  server writes. What only an operator reads, a declaration error or a log line, stays English
  in the code.

## Tests

- A test pins a behavior that a plausible bug breaks. Derive the expected value from the intended
  behavior, never by re-running the implementation.
- Assert structure and state (row counts, redirects, link targets, database rows), not wording,
  through the `tablo-test` queries (`rows`, `groups`, `row_actions`, `confirms_first`,
  `field_error`, `filter_options`, `empty_table`, `notification`). When no query reads what a test
  needs, add one over a `data-` hook rather than grep Tablo's markup or copy. Post a form with
  `TestClient::submit`, which carries the CSRF pair.
- A behavior is pinned once, in the one tier whose row below names it. Delete a test that proves
  nothing, including one that checks only how markup looks: classes, widths, layout order,
  decorative attributes, copy. A whole-body check stays for what must never appear, such as a
  leaked value or unescaped input.

  | Tier | Pins | Lives in |
  | --- | --- | --- |
  | unit | a function's result, a derive's expansion, a failure the public API cannot cause | `tests.rs` beside its source |
  | compile-fail | a misuse the type system refuses | `crates/tablo-core/tests/compile_fail/` |
  | showcase | what a request does to the showcase's resources | `examples/showcase/tests/` |
  | integration | what a request does to a panel the showcase does not build, what a mount refuses, what a derived impl does in an app's crate, the upstream behavior Tablo rests on | `integration/tests/` |
  | example | an example's own code: the guide's snippets, the quickstart's stylesheet | `examples/*/` |
  | guard | the repo's rules: the core's layers, the shipped catalogs, file sizes, the primitives copy | `crates/tablo-core/tests/layers.rs`, `crates/tablo-core/tests/catalogs.rs`, `xtask/tests/` |

  What a panel built from the public API does is pinned through a request, in the showcase or
  integration suite. A unit test may serve a request only to cause what no app can, such as a
  driver error mid-write.
- An invariant over a parser or an encoder is a `proptest` property, not a hand-picked input list.
- Unit tests live in `tests.rs` beside their source file (`foo/tests.rs` for `foo.rs`), and build
  their databases through `test_support` (`memory_db`, `tableless_db`).
- Both HTTP suites build their databases with `tablo::testing::test_db`: in-memory SQLite, or the
  database `TABLO_TEST_DATABASE_URL` names with the matching feature. CI runs them on PostgreSQL,
  MySQL and Turso too:

  ```sh
  TABLO_TEST_DATABASE_URL=postgresql://postgres:postgres@localhost/tablo \
    cargo test -p showcase --features postgresql --test it
  TABLO_TEST_DATABASE_URL=turso::memory: cargo test -p tablo-integration --features turso --test it
  ```

  A test connects to `sqlite::memory:` itself only when it touches no table: a database that lends
  only its schema, or a failure that needs the tables missing.

  An order or a match the test computes in Rust holds on every driver only when collation cannot
  change it.
- `examples/showcase/tests/it.rs` is one test binary whose modules are the suite files, one
  framework feature each: `cargo test -p showcase --test it list::` runs one file. `gates` holds the
  anonymous, CSRF and cross-tenant matrices, so a new route joins them there.
- `integration/tests/it.rs` covers what the showcase's resources do not reach, with test-local
  models on their own databases: `cargo test -p tablo-integration --test it upload::` runs one
  file. It is its own crate so that no published crate dev-depends on `tablo-test`.
- A suite file that pins one feature takes the name of the feature's source module in every tier:
  `panel/shell.rs` is pinned by `shell` in both suites, `auth/login.rs` by `login`. A feature split
  over several files shares a prefix (`action_input`, `action_policy`), and a file that crosses
  features names what it crosses (`gates`, `key_collisions`).

## Writing

Behavior is documented once, in rustdoc: what an item does, its rules, limits and errors. The
guide (`docs/guide/`) teaches: it builds a panel step by step, says why each part exists and how
the parts combine, and links to the rustdoc for the full rules rather than restating them. A
chapter passage that restates rustdoc becomes a link; a rule only the guide states moves into
rustdoc when its chapter is next edited. Vocabulary is in `CONTEXT.md`, and decisions in
`docs/adr/`. `README.md` is the entry point.

- Verify every claim against the code before writing it.
- Document current behavior only: no history, no plans. History belongs in a commit message.
- Active voice, present tense. Show the call or the output rather than describing it.
- No filler, hype, weasel words or metaphors: say "by default", not "out of the box"; say what the
  code does, not "under the hood" or "magic".
- A comment explains why: an invariant, an upstream workaround, a safety argument. It never
  restates the next line.
- An ADR records one decision and the alternatives it rejected, not the behavior that follows
  from it, and is amended in place. A retired number is never reused.

## Review

These are what review sends back most often.

- Fix the class, not the instance. A guard on one page covers every page with the same effect:
  list, detail, export, global search, relation table, import. A change to a shared helper is
  checked at every caller.
- Security checks fail closed: a missing tenant, policy, session or CSRF token refuses the
  request, never falls back to a laxer default.
- Delete code in the PR that makes it dead: the old path, the helper with no caller left, the
  field nothing reads. Tablo is experimental, so there is no compatibility shim to keep.
- A workaround that needs a paragraph of comment to justify it means the code is wrong. Fix the
  code.

## Pull requests and commits

Every branch is squash-merged into `master`, so the PR title becomes the commit: a Conventional
Commit under 100 characters, ending with the issue it closes, if any. A PR carries one concern; a
title that joins two changes with "and" is two PRs.

```
fix(table): bound the filters signal (#205)
```

- Types: `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `ci`, `build`, `chore`, `revert`.
- The scope is the module or crate the change lives in: a top-level `tablo-core` module (`table`,
  `schema`, `panel`, `auth`, `resource`, …) or `ui`, `macros`, `test`, `build`, `guide`,
  `showcase`, `bench`, `xtask`, plus `deps`, `release` and `repo`. A change across several
  modules takes the one it is about, or none; `core` names no module.
- A breaking change adds `!` after the scope and a `BREAKING CHANGE:` footer. It breaks callers
  when it removes or changes a public item, or adds an enum variant, a required trait method or a
  public field.
- The body says what changed and why. The PR description adds which checks ran.

## Issues

File through a form in `.github/ISSUE_TEMPLATE/`; it sets the label, and maintainers set the rest
from `.github/labels.yml`. An upstream gap is one missing Toasty or Topcoat API per issue, and its
body is its status: edit the body, do not comment.

By contributing, you agree that your contributions are licensed under the [MIT license](LICENSE).
