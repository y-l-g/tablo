use super::*;

/// The strip that makes the Rust half mean something: a hook kept alive only
/// by an assertion, a doc comment or a trailing annotation must not count.
#[test]
fn production_sources_drops_test_modules_and_comment_lines() {
    let src = r#"
attrs: attributes! { data-real-hook="" },

#[cfg(test)]
mod tests {
    #[test]
    fn marks_the_hook() {
        assert!(html.contains("data-test-only-hook"));
        // data-comment-only-hook
    }
}

// A trailing note about data-line-comment-hook.
fn after() { render("data-after-hook") }
"#;
    let stripped = production_sources(src);
    assert!(
        contains_hook(&stripped, "data-real-hook"),
        "a render-site hook must survive the strip: {stripped}"
    );
    assert!(
        contains_hook(&stripped, "data-after-hook"),
        "code after a test module must survive it: {stripped}"
    );
    assert!(
        !contains_hook(&stripped, "data-test-only-hook"),
        "a hook named only in an assertion must not count: {stripped}"
    );
    assert!(
        !contains_hook(&stripped, "data-comment-only-hook"),
        "a hook named only in an indented comment must not count: {stripped}"
    );
    assert!(
        !contains_hook(&stripped, "data-line-comment-hook"),
        "a hook named only in a whole-line comment must not count: {stripped}"
    );
}

/// A test module is cut by brace nesting, not by "everything after the
/// attribute" — otherwise one `#[cfg(test)]` would hide the rest of the
/// file and every hook below it would read as retired.
#[test]
fn production_sources_keeps_code_after_a_nested_test_module() {
    let src = "fn a() {}\n#[cfg(test)]\nmod tests {\n    fn b() { let x = \"}\"; }\n}\nfn tail() { \"data-tail\" }\n";
    let stripped = production_sources(src);
    assert!(
        contains_hook(&stripped, "data-tail"),
        "the module must close at its own brace: {stripped}"
    );
    assert!(
        !stripped.contains("mod tests"),
        "module must be gone: {stripped}"
    );
}

/// A bodiless `#[cfg(test)]` item ends at its `;`, not at the next item's
/// closing brace: `mod tests;` must not take the function below it along.
#[test]
fn production_sources_cuts_a_bodiless_test_item_at_its_semicolon() {
    let src = "#[cfg(test)]\nmod test_support;\nfn render() { \"data-kept\" }\n#[cfg(test)]\nconst P: &str = \"data-test-only\";\n";
    let stripped = production_sources(src);
    assert!(
        contains_hook(&stripped, "data-kept"),
        "code after a bodiless test item must survive it: {stripped}"
    );
    assert!(
        !contains_hook(&stripped, "data-test-only"),
        "the bodiless test item itself must be gone: {stripped}"
    );
}

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

/// The layers parser reads both the inline and the expanded tuple layout, and starts after the
/// `=` so the tuple type's own parentheses do not read as a layer.
#[test]
fn declared_layers_reads_both_layouts_past_the_type() {
    let src = r#"
const LAYERS: &[(&str, &[&str])] = &[
    (
        "foundations",
        &[
            "csrf",
            "protocol",
        ],
    ),
    ("resources", &["resource"]),
];
"#;
    assert_eq!(
        declared_layers(src),
        Some(vec![
            (
                "foundations".to_string(),
                vec!["csrf".to_string(), "protocol".to_string()]
            ),
            ("resources".to_string(), vec!["resource".to_string()]),
        ])
    );
    assert_eq!(declared_layers("const OTHER: () = ();"), None, "no LAYERS");
}

/// The table parser reads the layer rows only: the crate table above the heading and the rows of
/// the section below it are not layer rows.
#[test]
fn documented_layers_reads_only_the_layer_table() {
    let src = r#"
## Crates

| Crate | Depends on | Contents |
| --- | --- | --- |
| `tablo` | `tablo-core` | the facade |

### Inside `tablo-core`

| Layer | Modules |
| --- | --- |
| foundations | `csrf`, `protocol` |
| resources | `resource` |

## Requests

| Layer | Modules |
| --- | --- |
| not-a-row | `wrong` |
"#;
    assert_eq!(
        documented_layers(src),
        Some(vec![
            (
                "foundations".to_string(),
                vec!["csrf".to_string(), "protocol".to_string()]
            ),
            ("resources".to_string(), vec!["resource".to_string()]),
        ])
    );
}
