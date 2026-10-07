//! Mirrors [`VENDORED_PRIMITIVES`] into `primitives/` verbatim under SYNC headers.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

pub mod gates;

use topcoat_ui::{Component, Dependency, Registry};

/// The registry components Tablo vendors into `primitives/`; add a name here and run `cargo xtask
/// sync-topcoat-ui`.
pub const VENDORED_PRIMITIVES: &[&str] = &[
    "alert",
    "alert_dialog",
    "button",
    "card",
    "checkbox",
    "dialog",
    "field",
    "input",
    "label",
    "pagination",
    "select",
    "separator",
    "sheet",
    "sidebar",
    "skeleton",
    "table",
    "textarea",
];

fn vendored_components(registry: &Registry) -> anyhow::Result<Vec<Component<'_>>> {
    let components: Vec<Component<'_>> = VENDORED_PRIMITIVES
        .iter()
        .map(|name| {
            registry.get(name).ok_or_else(|| {
                anyhow::anyhow!(
                    "topcoat-ui-registry no longer offers `{name}`; update VENDORED_PRIMITIVES"
                )
            })
        })
        .collect::<anyhow::Result<_>>()?;
    for component in &components {
        for dependency in component.dependencies() {
            let Dependency::Same(name) = dependency else {
                continue;
            };
            if !VENDORED_PRIMITIVES.contains(&name.as_str()) {
                anyhow::bail!(
                    "`{}` depends on `{name}`, which is not in VENDORED_PRIMITIVES; add it",
                    component.name()
                );
            }
        }
    }
    Ok(components)
}

fn vendored_files(components: &[Component<'_>]) -> HashSet<String> {
    let mut files: HashSet<String> = components
        .iter()
        .map(|component| component.file_name().to_string())
        .collect();
    files.insert("mod.rs".to_string());
    files
}

/// Prepends to every synced file, where `hash` is the registry source's `sha256:` content hash.
fn sync_header(version: &str, hash: &str) -> String {
    format!(
        "// SYNC: topcoat-ui-registry@{version} {hash} — do not hand-edit. Sync via `cargo xtask sync-topcoat-ui` (ADR-0007).\n"
    )
}

fn mod_header(version: &str) -> String {
    format!(
        "// SYNC: topcoat-ui-registry@{version} — generated from the registry manifest. Sync via `cargo xtask sync-topcoat-ui` (ADR-0007).\n"
    )
}

pub fn primitives_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap_or(Path::new("."))
        .join("crates/tablo-ui/src/components/primitives")
}

/// Whether `file` delimits `anchor` with an `ANCHOR`/`ANCHOR_END` marker
/// pair. Both markers must name the anchor exactly: a renamed opener would
/// otherwise prefix-match and render empty without failing the book build.
fn has_anchor(file: &str, anchor: &str) -> bool {
    let (mut open, mut close) = (false, false);
    for line in file.lines() {
        let line = line.trim();
        if *line == format!("// ANCHOR: {anchor}") {
            open = true;
        }
        if *line == format!("// ANCHOR_END: {anchor}") {
            close = true;
        }
    }
    open && close
}

/// One `{{#include path}}` or `{{#include path:anchor}}` in a guide chapter.
fn guide_include(line: &str) -> Option<(String, Option<String>)> {
    let at = line.find("{{#include")?;
    let rest = line[at + "{{#include".len()..].strip_prefix(' ')?;
    let end = rest.find("}}")?;
    let target = rest[..end].trim();
    let (target, anchor) = match target.find(':') {
        Some(at) => (target[..at].to_string(), Some(target[at + 1..].to_string())),
        None => (target.to_string(), None),
    };
    Some((target, anchor))
}

/// Every guide include fails closed: `mdbook build` renders a missing file or
/// anchor as empty without failing, so this resolves each one against the
/// tree instead.
pub fn verify_guide_includes() -> anyhow::Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.parent().unwrap_or(Path::new("."));
    let guide = root.join("docs/guide/src");
    let mut failures = Vec::new();
    let mut chapters: Vec<_> = std::fs::read_dir(&guide)
        .map_err(|error| anyhow::anyhow!("cannot list {}: {error}", guide.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    chapters.sort();
    for chapter in &chapters {
        let src = std::fs::read_to_string(chapter)
            .map_err(|error| anyhow::anyhow!("cannot read {}: {error}", chapter.display()))?;
        for (line, text) in src.lines().enumerate() {
            let Some((target, anchor)) = guide_include(text) else {
                continue;
            };
            let path = chapter.parent().unwrap_or(Path::new(".")).join(&target);
            let Ok(file) = std::fs::read_to_string(&path) else {
                failures.push(format!(
                    "{}:{} includes {}, which cannot be read",
                    chapter.display(),
                    line + 1,
                    path.display()
                ));
                continue;
            };
            if let Some(anchor) = anchor
                && !has_anchor(&file, &anchor)
            {
                failures.push(format!(
                    "{}:{} includes :{anchor} of {}, which names no such anchor",
                    chapter.display(),
                    line + 1,
                    path.display()
                ));
            }
        }
    }
    if failures.is_empty() {
        println!("verified: every guide include resolves to an anchored source");
        Ok(())
    } else {
        anyhow::bail!("guide include drift detected:\n{}", failures.join("\n"));
    }
}

/// Loads the registry Cargo resolved for this workspace, plus its crate version.
fn locate_registry() -> anyhow::Result<(Registry, String)> {
    // Anchored at xtask's own manifest so a detached workspace never resolves the caller's CWD.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let output = std::process::Command::new("cargo")
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(&manifest)
        .output()
        .map_err(|error| anyhow::anyhow!("failed to run cargo metadata: {error}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| anyhow::anyhow!("could not parse cargo metadata: {error}"))?;

    let package = metadata["packages"]
        .as_array()
        .and_then(|packages| {
            packages
                .iter()
                .find(|package| package["name"] == "topcoat-ui-registry")
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "`topcoat-ui-registry` is not in the dependency graph — it must be a \
                 dependency of xtask (see xtask/Cargo.toml)"
            )
        })?;

    let version = package["version"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("topcoat-ui-registry has no version in cargo metadata"))?
        .to_string();
    let manifest_path = package["manifest_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("topcoat-ui-registry has no manifest_path"))?;
    let relative = package["metadata"]["topcoat-ui"]["registry"]
        .as_str()
        .unwrap_or(".");
    let dir = Path::new(manifest_path)
        .parent()
        .unwrap_or(Path::new("."))
        .join(relative);

    let registry = Registry::load(dir)?;
    Ok((registry, version))
}

/// What to run when a vendored file has drifted from the registry.
const HINT: &str = "run `cargo xtask sync-topcoat-ui` to restore the verbatim copy";

/// Copies every component in [`VENDORED_PRIMITIVES`] into `primitives/` verbatim under a SYNC
/// header, then regenerates `mod.rs`.
///
/// `prune` also deletes vendored files absent from the vendored set; without it, orphans are only
/// reported.
pub fn sync_topcoat_ui(dry_run: bool, prune: bool) -> anyhow::Result<()> {
    let dst_dir = primitives_dir();
    std::fs::create_dir_all(&dst_dir)?;

    let (registry, version) = locate_registry()?;
    let components = vendored_components(&registry)?;

    let mut count = 0;
    for component in &components {
        let src = component.read_source()?;
        let header = sync_header(&version, &topcoat_ui::content_hash(&src));
        let dst_path = dst_dir.join(component.file_name());
        if dry_run {
            println!("would sync {} -> {}", component.name(), dst_path.display());
        } else {
            std::fs::write(&dst_path, format!("{header}{src}"))?;
            println!("synced {}", component.name());
        }
        count += 1;
    }
    if dry_run {
        println!("dry-run: {count} components would be synced (topcoat-ui-registry@{version})");
    } else {
        println!(
            "done: {count} components synced to {} (topcoat-ui-registry@{version})",
            dst_dir.display()
        );
        println!("note: composites/ was not touched (ADR-0007)");
    }
    ensure_primitives_mod(&dst_dir, &version, &components, dry_run)?;
    if prune {
        prune_orphans(&dst_dir, &components, dry_run)?;
    }
    Ok(())
}

/// Deletes vendored files absent from [`VENDORED_PRIMITIVES`], never pruning the regenerated
/// `mod.rs`; dry runs only report.
fn prune_orphans(
    dst_dir: &Path,
    components: &[Component<'_>],
    dry_run: bool,
) -> anyhow::Result<()> {
    let expected = vendored_files(components);
    let mut orphans: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dst_dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with('.') {
                continue;
            }
            if !expected.contains(&file_name) {
                orphans.push(entry.path());
            }
        }
    }
    orphans.sort();
    for orphan in orphans {
        if dry_run {
            println!("would delete orphan {}", orphan.display());
        } else {
            std::fs::remove_file(&orphan)?;
            println!("deleted orphan {}", orphan.display());
        }
    }
    Ok(())
}

fn ensure_primitives_mod(
    dst_dir: &Path,
    version: &str,
    components: &[Component<'_>],
    dry_run: bool,
) -> anyhow::Result<()> {
    let mod_path = dst_dir.join("mod.rs");
    let mut content = mod_header(version);
    for component in components {
        content.push_str(&format!("pub mod {};\n", component.name()));
    }
    if dry_run {
        println!("would write {}", mod_path.display());
    } else {
        std::fs::write(&mod_path, content)?;
        println!("wrote {}", mod_path.display());
    }
    Ok(())
}

/// Fails when any vendored primitive has drifted from the registry.
pub fn verify_sync() -> anyhow::Result<()> {
    let dst_dir = primitives_dir();
    let (registry, version) = locate_registry()?;
    let components = vendored_components(&registry)?;

    let mut failures = Vec::new();

    for component in &components {
        let src = component.read_source()?;
        let expected = sync_header(&version, &topcoat_ui::content_hash(&src)) + &src;
        let dst_path = dst_dir.join(component.file_name());
        let installed = match std::fs::read_to_string(&dst_path) {
            Ok(content) => content,
            Err(error) => {
                failures.push(format!(
                    "{} cannot be read: {error}; {HINT}",
                    dst_path.display()
                ));
                continue;
            }
        };
        if installed == expected {
            continue;
        }
        let header_matches = installed
            .strip_prefix("// SYNC: topcoat-ui-registry@")
            .is_some_and(|rest| {
                rest.split_once(" — do not hand-edit.")
                    .is_some_and(|(head, _)| {
                        head.split_once(' ').is_some_and(|(ver, hash)| {
                            ver == version && hash == topcoat_ui::content_hash(&src)
                        })
                    })
            });
        if header_matches {
            failures.push(format!(
                "{} was hand-edited — it no longer matches the registry source; {HINT}",
                dst_path.display()
            ));
        } else {
            failures.push(format!(
                "{} carries a stale SYNC header or drifted content (topcoat-ui-registry@{version}); {HINT}",
                dst_path.display()
            ));
        }
    }

    let mod_path = dst_dir.join("mod.rs");
    let mut expected = mod_header(&version);
    for component in &components {
        expected.push_str(&format!("pub mod {};\n", component.name()));
    }
    match std::fs::read_to_string(&mod_path) {
        Ok(actual) if actual == expected => {}
        Ok(_) => failures.push(format!(
            "{} does not match the vendored set; {HINT}",
            mod_path.display()
        )),
        Err(error) => failures.push(format!(
            "{} cannot be read: {error}; {HINT}",
            mod_path.display()
        )),
    }

    // A component that left the vendored set must not linger as a stale file that still compiles
    // when referenced.
    {
        let expected_files = vendored_files(&components);
        if let Ok(entries) = std::fs::read_dir(&dst_dir) {
            let mut orphans: Vec<String> = Vec::new();
            for entry in entries.flatten() {
                let file_name = entry.file_name().to_string_lossy().to_string();
                if file_name.starts_with('.') {
                    continue;
                }
                if !expected_files.contains(&file_name) {
                    orphans.push(dst_dir.join(&file_name).display().to_string());
                }
            }
            orphans.sort();
            for orphan in orphans {
                failures.push(format!(
                    "{orphan} is not in the vendored set (orphaned vendored file); delete it or {HINT}"
                ));
            }
        }
    }

    if failures.is_empty() {
        println!(
            "verified: {} primitives match topcoat-ui-registry@{version} verbatim",
            components.len()
        );
        Ok(())
    } else {
        anyhow::bail!("registry drift detected:\n{}", failures.join("\n"));
    }
}

pub fn verify_vendored_closure() -> anyhow::Result<()> {
    let (registry, _version) = locate_registry()?;
    let components = vendored_components(&registry)?;
    println!(
        "verified: {} vendored primitives are closed under their registry dependencies",
        components.len()
    );
    Ok(())
}

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
];

/// One hook-contract entry: `js` appears in the asset's source and `rust` appears in the Rust
/// render sources.
pub struct AssetHook {
    pub asset: &'static str,
    pub js: &'static str,
    pub rust: &'static str,
}

/// Track new hooks here as they land.
pub const ASSET_HOOKS: &[AssetHook] = &[
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
        js: "sidebar_state",
        rust: "sidebar_state",
    },
    AssetHook {
        asset: "theme.js",
        js: "data-theme-toggle",
        rust: "data-theme-toggle",
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
