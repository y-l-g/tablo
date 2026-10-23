use super::*;

/// An include resolves to a file, and an anchored include to a delimited
/// anchor pair; anything else renders empty without failing the book build.
#[test]
fn guide_include_splits_target_and_anchor() {
    assert_eq!(
        guide_include("{{#include ../../examples/guide/src/tables.rs:table-format}}"),
        Some((
            "../../examples/guide/src/tables.rs".to_string(),
            Some("table-format".to_string())
        ))
    );
    assert_eq!(guide_include("```rust"), None, "a fence is not an include");
}

/// The live tree backs this: every guide include resolves, so a renamed
/// anchor or moved file fails `cargo test` instead of rendering empty.
#[test]
fn guide_includes_resolve_in_tree() {
    if let Err(error) = verify_guide_includes() {
        panic!("{error}");
    }
}
