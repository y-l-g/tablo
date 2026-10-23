# Tablo — Agent Instructions

[`CONTRIBUTING.md`](CONTRIBUTING.md) holds the rules for code, tests, prose, commits and issues,
and the commands to run. Read it before the first change. This file adds what applies to agents
only.

1. Run `cargo test --workspace --locked` on the merged result: branches can merge cleanly and not
   compile.
2. Give each worktree its own target directory; a shared `CARGO_TARGET_DIR` cross-contaminates.
3. Never pipe a command whose exit code you need: `| tail` masks it. Redirect to a file instead.
4. Use `gh` for issues and PRs. A bare `#123` is an issue or a PR: try `gh pr view 123`, then
   `gh issue view 123`.
5. When your change contradicts an ADR, say so in the PR instead of silently diverging.
6. Renovate groups coupled bumps (`renovate.json`). The lockfile carries `syn` 2 and 3 through
   upstream crates; do not force-unify them.
7. Report what you ran and what you did not. Never claim a check passed that you did not run.
