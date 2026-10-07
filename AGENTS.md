# Tablo — Rules

Every change follows these rules: code, tests, prose, commits, issues. Behaviour lives in the
code, the [guide](docs/guide/) and rustdoc; this file says what to do, not why. The commands
behind a rule are in [`CONTRIBUTING.md`](CONTRIBUTING.md#the-gate-set).

## Verify

1. Run the gates for the area you touched; `cargo xtask check --all` before merging.
2. Run `cargo test --workspace --locked` on the merged result: branches merge cleanly and still
   fail to compile.
3. Give each worktree its own target directory; a shared `CARGO_TARGET_DIR` cross-contaminates.
4. Never pipe when you need the exit code (`| tail` masks it): read `PIPESTATUS` or redirect to
   a file.
5. `topcoat fmt` reflows `view!` markup differently per release; only the pinned `topcoat-cli`
   matches CI, and its diff is not a hand-fix.
6. `cargo fmt` covers workspace members only; the detached `examples/quickstart` package is
   formatted by manifest path.
7. Never hand-edit `crates/tablo-ui/src/components/primitives/`; sync it with
   `cargo xtask sync-topcoat-ui`.

## Code

8. Verify every factual claim in a doc, comment or commit message against the code.
9. Verify upstream (Topcoat, Toasty) APIs against the version `Cargo.lock` pins, quoting the
   source file and symbol; never from memory.
10. Document current behaviour only: no history, no "used to", no roadmap.
11. A comment earns its place by explaining why — a non-obvious invariant, a named upstream bug,
    a safety argument — never by restating what the next line plainly does.
12. A struct is followed by its inherent impl, then its trait impls; unit tests go last, in
    `tests.rs` beside the source file.
13. Name a module's file after the module and place it beside its directory (`foo.rs` next to
    `foo/`), never `mod.rs`; existing `mod.rs` files stay.
14. Declare shared dependency versions in the workspace `[workspace.dependencies]`; crates pull
    them in with `workspace = true`.
15. Never run a blanket `cargo update`; a `topcoat`/`toasty` bump edits both manifests and the
    lockfile in one commit.
16. Hunt dead code in the `pub` API, always-same-value config and test-only paths.
17. `unsafe_code` and `warnings` are denied; `too_many_lines` is capped in `clippy.toml`.

## Tests

18. One predicate, one home: a behaviour is pinned once, by a unit test or an integration test,
    never both. Grep for the behaviour before pinning it.
19. Protect the behaviour, not its wording: assert structure, redirects, database state and link
    targets rather than messages and labels, so a passing rename does not break the suite.
20. Derive expected values from the intended behaviour, never by repeating the implementation or
    calling the code under test.
21. Delete a test no plausible bug would fail.
22. An example app declares one `it` integration-test binary in its manifest, because Cargo
    otherwise discovers every file under `tests/` as its own target; unit tests live in
    `tests.rs` beside their source; the browser suites run with `node --test`; the repo guards
    live in `xtask/tests/it.rs`.

## Prose

23. Write documentation, READMEs, ADRs, pull request and issue bodies, code comments and commit
    bodies in active voice and present tense, describing what the thing is and does.
24. Cut filler, buzzwords, weasel words and metaphors: say what the code does instead of "under
    the hood", "out of the box", "first-class", "magic", "footgun" or code that "lands" or
    "ships". Every sentence carries information, and a concrete example beats a description.
25. Wrap prose near column 100; no commit line is longer than 100 characters.

## Land a change

26. Squash-merge every branch into `master`: one Conventional Commit per branch,
    `type(scope): subject (#123)`, with the issue in the subject when the change closes one.
27. Mark a breaking change with `!` after the type or scope and explain it in a
    `BREAKING CHANGE:` footer.
28. The pull request title becomes the landed commit; the body describes the net diff, not the
    branch's latest commit.
29. Issue bodies follow the forms in `.github/ISSUE_TEMPLATE/`; an `upstream` issue carries its
    status in the body, never in comments. GitHub shares one number space, so a bare `#123` can
    be an issue or a pull request.

## Know the project

30. Vocabulary is [`CONTEXT.md`](CONTEXT.md): use its words in code, issues and commits, and
    surface a conflict with a decision rather than silently overriding it.
31. Crate roles and the request flow are in [`README.md#layout`](README.md#layout); the
    `tablo-core` layering is enforced by `crates/tablo-core/tests/layers.rs`.
32. Decisions are in [`docs/adr/`](docs/adr/); the guide documents behaviour and rustdoc on each
    item is the contract.
