//! Gate orchestration: `fmt`, `external-check`, `check`.
//!
//! `check` is a local fail-fast convenience runner only, never a CI job.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Dated nightly carrying the rustfmt the workspace check enforces.
pub const NIGHTLY_FMT: &str = "nightly-2026-08-24";

/// MSRV floor (`Cargo.toml` rust-version).
pub const MSRV: &str = "1.98";

/// Detached bench workspaces, each with its own lockfile and fmt gate.
pub const DETACHED_BENCHES: &[&str] = &[
    "benchmarks/tablo",
    "benchmarks/axum-maud",
    "benchmarks/leptos",
];

/// The detached app `external-check` builds from outside the repository.
pub const QUICKSTART: &str = "examples/quickstart";

/// JS asset suites, named rather than globbed so a rename fails loudly.
pub const ASSET_SUITES: &[&str] = &[
    "crates/tablo-ui/assets/selects.test.js",
    "crates/tablo-ui/assets/bulk.test.js",
    "crates/tablo-ui/assets/wire.test.js",
    "crates/tablo-ui/assets/dialog.test.js",
    "crates/tablo-ui/assets/mutation-submit.test.js",
    "crates/tablo-ui/assets/notifications.test.js",
    "crates/tablo-ui/assets/filters.test.js",
    "crates/tablo-ui/assets/live-search.test.js",
    "examples/showcase/assets/media.test.js",
];

pub fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// Runs `prog` with `args` in `dir`, inheriting stdio; never pipes, so no status gets masked.
pub trait Runner {
    fn run(
        &self,
        prog: &str,
        args: &[&str],
        dir: Option<&Path>,
        env: &[(&str, &str)],
    ) -> anyhow::Result<()>;
}

pub struct RealRunner;

impl Runner for RealRunner {
    fn run(
        &self,
        prog: &str,
        args: &[&str],
        dir: Option<&Path>,
        env: &[(&str, &str)],
    ) -> anyhow::Result<()> {
        let mut command = Command::new(prog);
        command.args(args);
        if let Some(dir) = dir {
            command.current_dir(dir);
        }
        for (key, value) in env {
            command.env(key, value);
        }
        let status = command
            .status()
            .map_err(|error| anyhow::anyhow!("failed to run {prog}: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            anyhow::bail!("`{prog} {}` exited with {status}", args.join(" "))
        }
    }
}

/// The pinned-nightly workspace fmt check (CONTRIBUTING gate 3).
pub fn nightly_fmt(run: &dyn Runner, root: &Path) -> anyhow::Result<()> {
    let toolchain = format!("+{NIGHTLY_FMT}");
    run.run(
        "cargo",
        &[&toolchain, "fmt", "--all", "--", "--check"],
        Some(root),
        &[],
    )
}

/// Checks fmt for each detached bench and the quickstart.
pub fn detached_fmt(run: &dyn Runner, root: &Path) -> anyhow::Result<()> {
    for bench in DETACHED_BENCHES.iter().chain([&QUICKSTART]) {
        run.run(
            "cargo",
            &["fmt", "--", "--check"],
            Some(&root.join(bench)),
            &[],
        )?;
    }
    Ok(())
}

/// Where the pinned CLI install the `topcoat fmt` check needs lives.
const TOPCOAT_INSTALL: &str = "cargo install topcoat-cli --version 0.10.0 --locked --force";

/// The pinned-CLI `topcoat fmt` check plus diff guard (CONTRIBUTING gate 4).
pub fn topcoat_fmt(run: &dyn Runner, root: &Path) -> anyhow::Result<()> {
    run.run("topcoat", &["fmt"], Some(root), &[])
        .map_err(|error| {
            anyhow::anyhow!(
                "{error}\n`topcoat fmt` needs the CLI built from the pinned version: {TOPCOAT_INSTALL}"
            )
        })?;
    run.run("git", &["diff", "--exit-code"], Some(root), &[])
        .map_err(|error| {
            anyhow::anyhow!(
                "{error}\nA diff that only reflows `view!` markup means the CLI is the wrong version, not a hand-fix: {TOPCOAT_INSTALL}"
            )
        })
}

/// The formatting subset: nightly fmt, detached-bench fmt, pinned topcoat fmt.
pub fn fmt_check(run: &dyn Runner) -> anyhow::Result<()> {
    let root = repo_root();
    nightly_fmt(run, &root)?;
    detached_fmt(run, &root)?;
    topcoat_fmt(run, &root)
}

/// The directory `external-check` stages the quickstart in: outside the
/// repository, and the same path every run from one checkout, so the build
/// cache keeps its fingerprints. The name carries a hash of `root`, so two
/// worktrees checking at once never clear or build each other's stage.
pub fn external_stage(root: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hasher);
    std::env::temp_dir().join(format!("tablo-external-check-{:016x}", hasher.finish()))
}

/// Rewrites the quickstart manifest's relative crate paths to absolute paths under `root`.
pub fn absolute_crate_paths(manifest: &str, root: &Path) -> anyhow::Result<String> {
    const RELATIVE: &str = "path = \"../../crates/";
    if !manifest.contains(RELATIVE) {
        anyhow::bail!("the quickstart manifest names no `{RELATIVE}…` dependency");
    }
    // Forward slashes on every platform: Cargo accepts them, and a TOML basic
    // string would read a backslash as an escape.
    let root = root.to_string_lossy().replace('\\', "/");
    Ok(manifest.replace(RELATIVE, &format!("path = \"{root}/crates/")))
}

fn copy_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Stages the quickstart's sources out of the repository, seeded with the workspace lockfile.
pub fn stage_quickstart(root: &Path) -> anyhow::Result<PathBuf> {
    let source = root.join(QUICKSTART);
    let stage = external_stage(root);
    if stage.exists() {
        std::fs::remove_dir_all(&stage)
            .map_err(|error| anyhow::anyhow!("cannot clear {}: {error}", stage.display()))?;
    }
    copy_dir(&source.join("src"), &stage.join("src"))?;
    for file in ["build.rs", "styles.css"] {
        std::fs::copy(source.join(file), stage.join(file))
            .map_err(|error| anyhow::anyhow!("cannot copy {QUICKSTART}/{file}: {error}"))?;
    }
    let manifest = std::fs::read_to_string(source.join("Cargo.toml"))
        .map_err(|error| anyhow::anyhow!("cannot read {QUICKSTART}/Cargo.toml: {error}"))?;
    std::fs::write(
        stage.join("Cargo.toml"),
        absolute_crate_paths(&manifest, root)?,
    )?;
    std::fs::copy(root.join("Cargo.lock"), stage.join("Cargo.lock"))
        .map_err(|error| anyhow::anyhow!("cannot seed the lockfile: {error}"))?;
    println!("staged {QUICKSTART} at {}", stage.display());
    Ok(stage.join("Cargo.toml"))
}

/// Tests the staged quickstart: it builds against Tablo only through absolute
/// paths, generates its stylesheet, and asserts that stylesheet holds classes
/// only Tablo's own sources write.
pub fn external_check(run: &dyn Runner, root: &Path, manifest: &Path) -> anyhow::Result<()> {
    let target = root.join("target/external-check");
    let manifest = manifest.to_string_lossy();
    let target = target.to_string_lossy();
    run.run(
        "cargo",
        &["test", "--manifest-path", &manifest],
        Some(root),
        &[("CARGO_TARGET_DIR", &target)],
    )
}

/// The gate set as a local fail-fast convenience runner: the eight CONTRIBUTING
/// gates in order, then docs, detached-bench fmt, and the external build.
pub fn check(run: &dyn Runner) -> anyhow::Result<()> {
    check_with(run, &|| stage_quickstart(&repo_root()))
}

fn check_with(run: &dyn Runner, stage: &dyn Fn() -> anyhow::Result<PathBuf>) -> anyhow::Result<()> {
    let root = repo_root();
    run.run(
        "cargo",
        &["test", "--workspace", "--locked"],
        Some(&root),
        &[],
    )?;
    run.run(
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
        Some(&root),
        &[],
    )?;
    nightly_fmt(run, &root)?;
    topcoat_fmt(run, &root)?;
    run.run(
        "cargo",
        &[
            "clippy",
            "--locked",
            "--manifest-path",
            "benchmarks/tablo/Cargo.toml",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        Some(&root),
        &[],
    )?;
    let msrv = format!("+{MSRV}");
    run.run(
        "cargo",
        &[&msrv, "check", "--workspace", "--locked"],
        Some(&root),
        &[],
    )?;
    let mut assets: Vec<&str> = vec!["--test"];
    assets.extend_from_slice(ASSET_SUITES);
    run.run("node", &assets, Some(&root), &[])?;
    run.run(
        "cargo",
        &["+nightly", "install", "cargo-udeps", "--locked"],
        Some(&root),
        &[],
    )?;
    run.run(
        "cargo",
        &[
            "+nightly",
            "udeps",
            "--workspace",
            "--all-targets",
            "--locked",
        ],
        Some(&root),
        &[],
    )?;
    run.run(
        "cargo",
        &["doc", "--workspace", "--no-deps", "--locked"],
        Some(&root),
        &[("RUSTDOCFLAGS", "-D warnings")],
    )?;
    run.run("mdbook", &["build", "docs/guide"], Some(&root), &[])?;
    detached_fmt(run, &root)?;
    external_check(run, &root, &stage()?)
}

#[cfg(test)]
mod tests;
