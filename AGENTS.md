# Tablo — Agent Instructions

[`CONTRIBUTING.md`](CONTRIBUTING.md) holds the rules for code, tests, prose, commits and issues.
Read it before the first change. This file adds what applies to agents only.

```sh
cargo xtask check          # before every commit
cargo xtask check --all    # before merging
topcoat dev -p showcase    # http://localhost:3000/admin/users
```

1. Verify every claim in a doc, comment, commit or PR against the code, and every upstream API
   against the source of the version `Cargo.lock` pins.
2. Run `cargo test --workspace --locked` on the merged result: branches can merge cleanly and not
   compile.
3. Give each worktree its own target directory; a shared `CARGO_TARGET_DIR` cross-contaminates.
4. Never pipe a command whose exit code you need: `| tail` masks it. Redirect to a file instead.
5. A `topcoat fmt` diff from an unpinned CLI is not a fix; install the version `cargo xtask fmt`
   names.
6. Use `gh` for issues and PRs. A bare `#123` is an issue or a PR: try `gh pr view 123`, then
   `gh issue view 123`.
7. When your change contradicts an ADR, say so in the PR instead of silently diverging.
8. Renovate groups coupled bumps (`renovate.json`). The lockfile carries `syn` 2 and 3 through
   upstream crates; do not force-unify them.
