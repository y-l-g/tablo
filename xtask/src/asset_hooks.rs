//! The hook contract between the shell's JS assets and the Rust that renders their markup.

use std::path::{Path, PathBuf};

/// The directory holding the hand-written shell JS assets.
pub fn assets_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap_or(Path::new("."))
        .join("crates/tablo-ui/assets")
}

/// Each shell JS asset's file under `assets/` and the `tablo-ui` constant wiring it into the
/// document head.
pub const ASSET_FILES: &[(&str, &str)] = &[
    ("sidebar.js", "SIDEBAR_JS"),
    ("theme.js", "THEME_JS"),
    ("selects.js", "SELECTS_JS"),
    ("repeaters.js", "REPEATERS_JS"),
    ("password.js", "PASSWORD_JS"),
    ("tabs.js", "TABS_JS"),
    ("forms.js", "FORMS_JS"),
    ("rich_text.js", "RICH_TEXT_JS"),
];

/// One hook-contract entry: `js` appears in the asset's source and `rust` appears in the Rust
/// render sources.
pub struct AssetHook {
    /// The asset file under `assets/` that consumes the hook.
    pub asset: &'static str,
    /// The needle that must appear in the asset's source.
    pub js: &'static str,
    /// The needle that must appear in the Rust sources.
    pub rust: &'static str,
}

/// Track new hooks here as they land.
pub const ASSET_HOOKS: &[AssetHook] = &[
    AssetHook {
        asset: "password.js",
        js: "data-password-toggle",
        rust: "data-password-toggle",
    },
    AssetHook {
        asset: "sidebar.js",
        js: "data-sidebar",
        rust: "data-sidebar",
    },
    AssetHook {
        asset: "sidebar.js",
        js: "data-state",
        rust: "data-state",
    },
    AssetHook {
        asset: "sidebar.js",
        js: "tablo_sidebar",
        rust: "tablo_sidebar",
    },
    AssetHook {
        asset: "theme.js",
        js: "data-theme-set",
        rust: "data-theme-set",
    },
    AssetHook {
        asset: "theme.js",
        js: "tablo_scheme",
        rust: "tablo_scheme",
    },
    AssetHook {
        asset: "theme.js",
        js: "tablo_theme",
        rust: "tablo_theme",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-select-filterable",
        rust: "data-select-filterable",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-options-filter",
        rust: "data-options-filter",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-options-field",
        rust: "data-options-field",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-options-server",
        rust: "data-options-server",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-options-combobox",
        rust: "data-options-combobox",
    },
    AssetHook {
        asset: "selects.js",
        js: "data-options-list",
        rust: "data-options-list",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater",
        rust: "data-repeater",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-next",
        rust: "data-repeater-next",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-order",
        rust: "data-repeater-order",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-rows",
        rust: "data-repeater-rows",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-row",
        rust: "data-repeater-row",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-blank",
        rust: "data-repeater-blank",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-add",
        rust: "data-repeater-add",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-remove",
        rust: "data-repeater-remove",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-up",
        rust: "data-repeater-up",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "data-repeater-down",
        rust: "data-repeater-down",
    },
    AssetHook {
        asset: "repeaters.js",
        js: "__row__",
        rust: "__row__",
    },
    AssetHook {
        asset: "tabs.js",
        js: "data-tabs",
        rust: "data-tabs",
    },
    AssetHook {
        asset: "tabs.js",
        js: "data-tab-panel",
        rust: "data-tab-panel",
    },
    AssetHook {
        asset: "tabs.js",
        js: "role=tablist",
        rust: "role=\"tablist\"",
    },
    AssetHook {
        asset: "tabs.js",
        js: "role=tab]",
        rust: "role=\"tab\"",
    },
    AssetHook {
        asset: "forms.js",
        js: "data-form-guard",
        rust: "data-form-guard",
    },
    AssetHook {
        asset: "forms.js",
        js: "data-form-dirty",
        rust: "data-form-dirty",
    },
    AssetHook {
        asset: "forms.js",
        js: "data-section",
        rust: "data-section",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text",
        rust: "data-rich-text",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "tablo-tiptap",
        rust: "tablo-tiptap",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text-tools",
        rust: "data-rich-text-tools",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text-toolbar",
        rust: "data-rich-text-toolbar",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text-tool",
        rust: "data-rich-text-tool",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text-content",
        rust: "data-rich-text-content",
    },
    AssetHook {
        asset: "rich_text.js",
        js: "data-rich-text-source",
        rust: "data-rich-text-source",
    },
];

/// Counts `needle` only when neither neighbor continues the name.
fn contains_hook(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.match_indices(needle).any(|(i, _)| {
        let before = haystack[..i].chars().next_back();
        let after = haystack[i + needle.len()..].chars().next();
        !before.is_some_and(is_hook_char) && !after.is_some_and(is_hook_char)
    })
}

/// Strips test modules and comment-only lines from source text.
fn production_sources(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "#[cfg(test)]".len()..];
        let mut depth = 0usize;
        let mut chars = after.char_indices();
        let mut end = after.len();
        let mut seen_open = false;
        for (i, c) in chars.by_ref() {
            match c {
                ';' if !seen_open => {
                    end = i + 1;
                    break;
                }
                '{' => {
                    depth += 1;
                    seen_open = true;
                }
                '}' => {
                    depth -= 1;
                    if seen_open && depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out.lines()
        .filter(|line| {
            let t = line.trim_start();
            !(t.starts_with("//") || t.starts_with("/*") || t.starts_with('*'))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rust_sources(root: &Path) -> anyhow::Result<String> {
    let mut out = String::new();
    for dir in ["crates/tablo-ui/src", "crates/tablo-core/src"] {
        let mut files = Vec::new();
        collect_rs(&root.join(dir), &mut files)?;
        for path in files {
            if path.file_name().is_some_and(|name| name == "tests.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path)?;
            out.push_str(&production_sources(&src));
            out.push('\n');
        }
    }
    Ok(out)
}

fn is_hook_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

const HOOK_HINT: &str = "update the hook list and both sides together";

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Fails when any shell JS asset or hook has drifted from the hook contract.
pub fn verify_asset_hooks() -> anyhow::Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent().unwrap_or(Path::new("."));
    let assets = assets_dir();
    let mut failures = Vec::new();

    let lib_rs = std::fs::read_to_string(root.join("crates/tablo-ui/src/lib.rs"))
        .map_err(|error| anyhow::anyhow!("cannot read tablo-ui/src/lib.rs: {error}"))?;

    let mut sources: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    for (file, constant) in ASSET_FILES {
        let path = assets.join(file);
        match std::fs::read_to_string(&path) {
            Ok(src) if !src.trim().is_empty() => {
                sources.insert(file, src);
            }
            Ok(_) => failures.push(format!("{file} is empty; {HOOK_HINT}")),
            Err(error) => failures.push(format!(
                "{} cannot be read: {error}; {HOOK_HINT}",
                path.display()
            )),
        }
        if !contains_hook(&lib_rs, constant) {
            failures.push(format!(
                "{constant} is gone from tablo-ui/src/lib.rs, so {file} is no longer wired into the document; {HOOK_HINT}"
            ));
        }
    }

    // The Rust half reads the stripped sources, so an assertion or doc comment cannot stand in for
    // the markup.
    let rust_src = match rust_sources(root) {
        Ok(sources) => sources,
        Err(error) => {
            failures.push(format!(
                "cannot read the Rust sources: {error}; {HOOK_HINT}"
            ));
            String::new()
        }
    };
    for hook in ASSET_HOOKS {
        match sources.get(hook.asset) {
            Some(src) if contains_hook(src, hook.js) => {}
            Some(_) => failures.push(format!(
                "{} no longer contains `{}`; {HOOK_HINT}",
                hook.asset, hook.js
            )),
            None => failures.push(format!(
                "{} is missing, so its `{}` hook cannot be checked; {HOOK_HINT}",
                hook.asset, hook.js
            )),
        }
        if !contains_hook(&rust_src, hook.rust) {
            failures.push(format!(
                "`{}` (consumed by {}) is gone from the Rust sources; {HOOK_HINT}",
                hook.rust, hook.asset
            ));
        }
    }

    if failures.is_empty() {
        println!(
            "verified: {} assets and {} hooks match the hook contract",
            ASSET_FILES.len(),
            ASSET_HOOKS.len()
        );
        Ok(())
    } else {
        anyhow::bail!("asset hook drift detected:\n{}", failures.join("\n"));
    }
}

#[cfg(test)]
mod tests;
