# Integration tests: one binary per crate, not per file

Date: 2026-09-19 — Status: accepted

## Decision

Both `examples/showcase` and `crates/tablo-core` set `autotests = false` and declare a single
`[[test]] name = "it"` target. `tests/it.rs` declares each former test file as a module sharing
one fixture, so the fixture compiles once per crate. Each per-file target costs ~160-190 MB of
artifacts linking the same server stack; the single target links it once. `xtask/tests/` keeps its
two files.

A file's tests are its module's: `cargo test --test it admin::`. Modules share one process;
nothing here mutates process-wide state, which makes that safe. A test needing isolation gets a
dedicated target.
