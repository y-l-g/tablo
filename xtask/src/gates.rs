//! Gate orchestration: `fmt`, `external-check`, `check`.
//!
//! `check` is a local fail-fast convenience runner only, never a CI job.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Dated nightly carrying the rustfmt the workspace check enforces.
pub const NIGHTLY_FMT: &str = "nightly-2026-08-24";

/// MSRV floor: the workspace `rust-version`, which xtask inherits.
pub const MSRV: &str = env!("CARGO_PKG_RUST_VERSION");

/// The detached app `external-check` builds from outside the repository.
pub const QUICKSTART: &str = "examples/quickstart";

/// Mirrors the `pull_request: paths` filter of `.github/workflows/msrv-udeps.yml`.
pub const MSRV_PATHS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "**/Cargo.toml",
    "**/Cargo.lock",
    "rust-toolchain.toml",
    ".github/workflows/msrv-udeps.yml",
];

/// Whether `check` skips the gates CI would not run for the change.
#[derive(PartialEq, Eq)]
pub enum Scope {
    /// The fast gates only: the MSRV/udeps gates without `MSRV_PATHS` changes stay skipped, and
    /// docs and the external build never run.
    Auto,
    /// Run every gate, including the slow docs and external builds.
    All,
}

/// JS asset suites, named rather than globbed so a rename fails loudly.
pub const ASSET_SUITES: &[&str] = &["crates/tablo-ui/assets/selects.test.js"];

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

/// Runs git in `root` and returns its stdout; `None` when git fails.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Whether the change under `root` touches any of `specs` (git pathspecs).
/// Combines the committed branch diff against the merge base with
/// `origin/master` (else `master`) and the uncommitted worktree state. Any git
/// failure returns true: a gate that cannot prove irrelevance runs.
pub fn changes_touch(root: &Path, specs: &[&str]) -> bool {
    let Some(base) = ["origin/master", "master"]
        .iter()
        .find_map(|branch| git(root, &["merge-base", branch, "HEAD"]))
    else {
        return true;
    };
    let base = base.trim();
    let mut committed: Vec<&str> = vec!["diff", "--name-only", &base, "HEAD", "--"];
    committed.extend_from_slice(specs);
    let mut uncommitted: Vec<&str> = vec!["status", "--porcelain", "--"];
    uncommitted.extend_from_slice(specs);
    git(root, &committed).is_none_or(|out| !out.trim().is_empty())
        || git(root, &uncommitted).is_none_or(|out| !out.trim().is_empty())
}

/// The pinned-nightly workspace fmt check.
pub fn nightly_fmt(run: &dyn Runner, root: &Path) -> anyhow::Result<()> {
    let toolchain = format!("+{NIGHTLY_FMT}");
    run.run(
        "cargo",
        &[&toolchain, "fmt", "--all", "--", "--check"],
        Some(root),
        &[],
    )
}

/// Checks fmt for the detached quickstart.
pub fn detached_fmt(run: &dyn Runner, root: &Path) -> anyhow::Result<()> {
    run.run(
        "cargo",
        &["fmt", "--", "--check"],
        Some(&root.join(QUICKSTART)),
        &[],
    )
}

/// Where the pinned CLI install the `topcoat fmt` check needs lives.
const TOPCOAT_INSTALL: &str = "cargo install topcoat-cli --version 0.10.0 --locked --force";

/// Where the pinned mdBook install the guide build needs lives.
const MDBOOK_INSTALL: &str = "cargo install mdbook --version 0.5.2 --locked --force";

/// The pinned-CLI `topcoat fmt` check plus diff guard.
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
                "{error}\n`topcoat fmt` rewrites the tree before this check: review it with `git diff --name-only` and revert what you did not mean to reformat with `git checkout -- <path>`. A diff that only reflows `view!` markup means the CLI is the wrong version, not a hand-fix: {TOPCOAT_INSTALL}"
            )
        })
}

/// The guide build (the CI `docs` job).
pub fn guide_build(run: &dyn Runner, root: &Path) -> anyhow::Result<()> {
    run.run("mdbook", &["build", "docs/guide"], Some(root), &[])
        .map_err(|error| anyhow::anyhow!("{error}\nThe guide build needs mdBook: {MDBOOK_INSTALL}"))
}

/// The formatting subset: nightly fmt, detached fmt, pinned topcoat fmt.
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

/// Whether `cargo +nightly udeps` already runs, so `check` skips the reinstall.
fn udeps_installed(run: &dyn Runner, root: &Path) -> bool {
    run.run(
        "cargo",
        &["+nightly", "udeps", "--version"],
        Some(root),
        &[],
    )
    .is_ok()
}

/// The CI checks as a local fail-fast runner, cheapest first. Docs and the external build run only
/// under `Scope::All`; CI covers them in parallel jobs. The MSRV/udeps gates run only
/// with `MSRV_PATHS` changes, matching the CI path filter.
pub fn check(run: &dyn Runner, scope: Scope) -> anyhow::Result<()> {
    let root = repo_root();
    check_with(run, &|| stage_quickstart(&root), scope, &|specs| {
        changes_touch(&root, specs)
    })
}

fn check_with(
    run: &dyn Runner,
    stage: &dyn Fn() -> anyhow::Result<PathBuf>,
    scope: Scope,
    touches: &dyn Fn(&[&str]) -> bool,
) -> anyhow::Result<()> {
    let root = repo_root();
    let all = scope == Scope::All;
    // Seconds first: a formatting or asset breakage fails before the minute-long builds.
    nightly_fmt(run, &root)?;
    topcoat_fmt(run, &root)?;
    let mut assets: Vec<&str> = vec!["--test"];
    assets.extend_from_slice(ASSET_SUITES);
    run.run("node", &assets, Some(&root), &[])?;
    detached_fmt(run, &root)?;
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
    if all || touches(MSRV_PATHS) {
        let msrv = format!("+{MSRV}");
        run.run(
            "cargo",
            &[&msrv, "check", "--workspace", "--locked"],
            Some(&root),
            &[],
        )?;
        if !udeps_installed(run, &root) {
            run.run(
                "cargo",
                &["+nightly", "install", "cargo-udeps", "--locked"],
                Some(&root),
                &[],
            )?;
        }
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
    } else {
        println!("skipped the MSRV check and udeps: no manifest or lockfile changed");
    }
    if all {
        run.run(
            "cargo",
            &["doc", "--workspace", "--no-deps", "--locked"],
            Some(&root),
            &[("RUSTDOCFLAGS", "-D warnings")],
        )?;
        guide_build(run, &root)?;
        external_check(run, &root, &stage()?)?;
    } else {
        println!("skipped docs and the external build: pass --all to run them");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
