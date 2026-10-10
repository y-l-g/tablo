//! Declarations the API refuses at compile time, each pinned by its compiler error.

#[test]
fn refused_declarations() {
    trybuild::TestCases::new().compile_fail("tests/compile_fail/*.rs");
}
