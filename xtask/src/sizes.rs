//! The file-size ratchet: a Rust source past its limit is a module waiting to be split, and
//! [`OVERSIZED`] names the ones that are already, each capped at its size when it was listed.

use std::path::{Path, PathBuf};

/// The most lines a production source holds.
pub const MAX_LINES: usize = 600;

/// The most lines a test source holds: a `tests.rs`, or a file under a `tests/` directory.
pub const MAX_TEST_LINES: usize = 1000;

/// The trees the ratchet walks, from the repository root.
const ROOTS: &[&str] = &["crates", "examples", "integration", "xtask"];

/// The vendored primitives: the registry writes them, so their size is not Tablo's to choose.
const VENDORED: &str = "crates/tablo-ui/src/components/primitives/";

/// The sources over their limit, each with the most lines it may hold. The list only shrinks:
/// a listed file may not grow past its cap, and one back under its limit leaves the list.
pub const OVERSIZED: &[(&str, usize)] = &[
    ("crates/tablo-core/src/auth/reset.rs", 807),
    ("crates/tablo-core/src/auth/tests.rs", 1053),
    ("crates/tablo-core/src/declaration.rs", 1085),
    ("crates/tablo-core/src/panel/build/tests.rs", 1252),
    ("crates/tablo-core/src/panel/shell.rs", 746),
    ("crates/tablo-core/src/schema/embedded.rs", 750),
    ("crates/tablo-core/src/schema/fields.rs", 906),
    ("crates/tablo-core/src/schema/fields/choice.rs", 643),
    ("crates/tablo-core/src/table.rs", 625),
    ("crates/tablo-core/src/table/tests.rs", 1048),
    ("crates/tablo-core/src/toasty_compat/model.rs", 635),
    ("integration/tests/record_form.rs", 1202),
    ("xtask/src/lib.rs", 796),
];

/// Whether `path`, relative to the root with `/` separators, is a test source.
fn is_test(path: &str) -> bool {
    path.ends_with("/tests.rs") || path.contains("/tests/")
}

/// The limit `path` answers to.
pub fn limit(path: &str) -> usize {
    if is_test(path) {
        MAX_TEST_LINES
    } else {
        MAX_LINES
    }
}

/// Every Rust source under [`ROOTS`] but the vendored ones, relative to `root`, skipping build
/// output and hidden directories.
fn sources(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut pending: Vec<PathBuf> = ROOTS.iter().map(|dir| root.join(dir)).collect();
    let mut found = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if path.is_dir() {
                if name != "target" && !name.starts_with('.') {
                    pending.push(path);
                }
            } else if name.ends_with(".rs") {
                let relative = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                if !relative.starts_with(VENDORED) {
                    found.push(relative);
                }
            }
        }
    }
    found.sort();
    Ok(found)
}

/// What breaks the ratchet for one file of `lines` lines, given its [`OVERSIZED`] cap.
pub fn violation(path: &str, lines: usize, cap: Option<usize>) -> Option<String> {
    let limit = limit(path);
    match cap {
        None if lines > limit => Some(format!(
            "{path} holds {lines} lines, past the {limit} limit: split it by concern"
        )),
        Some(cap) if lines > cap => Some(format!(
            "{path} holds {lines} lines, past its OVERSIZED cap of {cap}: split it instead"
        )),
        Some(_) if lines <= limit => Some(format!(
            "{path} holds {lines} lines, within the {limit} limit: drop it from OVERSIZED"
        )),
        _ => None,
    }
}

/// Every source within its limit or its [`OVERSIZED`] cap, and every entry of that list still
/// over its limit.
pub fn verify_file_sizes(root: &Path) -> anyhow::Result<()> {
    let sources = sources(root)?;
    let mut problems = Vec::new();
    for (path, _) in OVERSIZED {
        if !sources.iter().any(|source| source == path) {
            problems.push(format!(
                "{path} is in OVERSIZED but no longer exists: drop it"
            ));
        }
    }
    for path in &sources {
        let lines = std::fs::read_to_string(root.join(path))?.lines().count();
        let cap = OVERSIZED
            .iter()
            .find(|(listed, _)| listed == path)
            .map(|(_, cap)| *cap);
        problems.extend(violation(path, lines, cap));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("{}", problems.join("\n"))
    }
}
