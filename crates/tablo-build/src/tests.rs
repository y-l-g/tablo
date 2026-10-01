use std::path::{Path, PathBuf};

use super::{input_css, source_dirs};

#[test]
fn source_dirs_reads_the_facade_pair_and_each_crate() {
    let dirs = source_dirs(|name| match name {
        "DEP_TABLO_CORE" | "DEP_TABLO_CORE_SRC" => Some("/cargo/tablo-core/src".into()),
        "DEP_TABLO_UI" => Some("/cargo/tablo-ui/src".into()),
        _ => None,
    });
    assert_eq!(
        dirs,
        vec![
            PathBuf::from("/cargo/tablo-core/src"),
            PathBuf::from("/cargo/tablo-ui/src"),
        ],
        "a directory published twice is scanned once"
    );
}

#[test]
fn source_dirs_keeps_a_path_holding_a_separator_whole() {
    let dirs = source_dirs(|name| (name == "DEP_TABLO_CORE").then(|| "/odd:dir/src".into()));
    assert_eq!(dirs, vec![PathBuf::from("/odd:dir/src")]);
}

#[test]
fn source_dirs_is_empty_without_a_tablo_dependency() {
    assert!(source_dirs(|_| None).is_empty());
}

#[test]
fn input_imports_the_app_stylesheet_then_sources_each_directory() {
    let css = input_css(
        Path::new("/app/styles.css"),
        &[PathBuf::from("/cargo/tablo-core/src")],
    );
    assert_eq!(
        css,
        "@import \"/app/styles.css\";\n@source \"/cargo/tablo-core/src/**/*.rs\";\n"
    );
}

#[test]
fn input_escapes_quotes_in_a_path() {
    let css = input_css(Path::new("/a \"b\"/styles.css"), &[]);
    assert_eq!(css, "@import \"/a \\\"b\\\"/styles.css\";\n");
}
