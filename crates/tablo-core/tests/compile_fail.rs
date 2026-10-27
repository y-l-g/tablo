//! Declarations the API refuses at compile time, each pinned by its compiler error.
//!
//! Ignored by default: trybuild checks Tablo again in a target directory of its own, which takes
//! a minute after every tablo-core change. CI and `cargo xtask check --all` run it with
//! `cargo test -p tablo-core --test compile_fail -- --ignored`.

#[test]
#[ignore = "slow: run with `--ignored`"]
fn refused_declarations() {
    trybuild::TestCases::new().compile_fail("tests/compile_fail/*.rs");
}
