//! Gate orchestration: `fmt`, `bump-upstream`, `verify-locks`, `check`.
//!
//! Each CI job calls one subcommand instead of inlining its own bash loop, so
//! the loops are typed, tested, and runnable locally. Jobs stay parallel —
//! `check` is a local fail-fast convenience runner only, never a CI job.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};

/// Crates pinned in lockstep across the workspace and bench lockfiles (GH #103).
pub const LOCKSTEP_CRATES: &[&str] = &["topcoat", "toasty"];

/// Upstream repos pinned by `rev =` in both manifests, in the same order as
/// `set_upstream_revs`' rev arguments. The writer and the `verify-locks`
/// manifest guard match through this table, so they cannot disagree on what
/// a repo is.
pub const UPSTREAM_REPOS: &[(&str, &str)] = &[
    ("topcoat", "github.com/tokio-rs/topcoat"),
    ("toasty", "github.com/tokio-rs/toasty"),
];

/// Dated nightly carrying the rustfmt the workspace check enforces (GH #269).
pub const NIGHTLY_FMT: &str = "nightly-2026-08-24";

/// MSRV floor (`Cargo.toml` rust-version, GH #175).
pub const MSRV: &str = "1.98";

/// Detached bench workspaces, each with its own lockfile and fmt gate (GH #175).
pub const DETACHED_BENCHES: &[&str] = &[
    "benchmarks/tablo",
    "benchmarks/axum-maud",
    "benchmarks/leptos",
];

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

/// The repo root (xtask lives at `<root>/xtask`).
pub fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// How a gate command runs: `prog` with `args` in `dir`, plus extra `env`.
/// Commands inherit stdio and propagate their exit code — never through a
/// pipe, so no status gets masked (AGENTS.md rule 5).
pub trait Runner {
    fn run(
        &self,
        prog: &str,
        args: &[&str],
        dir: Option<&Path>,
        env: &[(&str, &str)],
    ) -> anyhow::Result<()>;
}

/// The real runner: inherit stdio, fail on a non-zero exit.
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

/// The detached-bench fmt checks: `cargo fmt` never sees workspace-excluded members.
pub fn detached_fmt(run: &dyn Runner, root: &Path) -> anyhow::Result<()> {
    for bench in DETACHED_BENCHES {
        run.run(
            "cargo",
            &["fmt", "--", "--check"],
            Some(&root.join(bench)),
            &[],
        )?;
    }
    Ok(())
}

/// Where the locked-rev CLI install the `topcoat fmt` check needs lives.
const TOPCOAT_INSTALL: &str = "REV=$(grep -A 2 '^name = \"topcoat\"$' Cargo.lock | grep -o '#[0-9a-f]\\{40\\}' | head -1 | cut -c2-) && cargo install --git https://github.com/tokio-rs/topcoat --rev \"$REV\" topcoat-cli --locked";

/// The locked-rev `topcoat fmt` check plus diff guard (CONTRIBUTING gate 4).
/// Check-half only: installing the CLI (~4 min build) stays in CI's cache-keyed
/// step, so a missing or wrong-rev CLI fails here with the install command.
pub fn topcoat_fmt(run: &dyn Runner, root: &Path) -> anyhow::Result<()> {
    run.run("topcoat", &["fmt"], Some(root), &[])
        .map_err(|error| {
            anyhow::anyhow!(
                "{error}\n`topcoat fmt` needs the CLI built from the locked rev: {TOPCOAT_INSTALL}"
            )
        })?;
    run.run("git", &["diff", "--exit-code"], Some(root), &[])
        .map_err(|error| {
            anyhow::anyhow!(
                "{error}\nA diff that only reflows `view!` markup means the CLI is the wrong rev, not a hand-fix: {TOPCOAT_INSTALL}"
            )
        })
}

/// The formatting subset: nightly fmt, detached-bench fmt, locked-rev topcoat fmt.
pub fn fmt_check(run: &dyn Runner) -> anyhow::Result<()> {
    let root = repo_root();
    nightly_fmt(run, &root)?;
    detached_fmt(run, &root)?;
    topcoat_fmt(run, &root)
}

/// The `source` field's resolved git sha (`git+https://…?rev=<sha>#<sha>`).
fn git_pin(source: &str) -> Option<&str> {
    source
        .rsplit_once('#')
        .map(|(_, sha)| sha)
        .filter(|sha| !sha.is_empty())
}

/// The lockstep pins parsed from one `cargo metadata` document (serde_json is
/// already an xtask dep, so no new `toml` dep risks the MSRV floor).
pub fn pins_from_metadata(metadata: &serde_json::Value) -> BTreeMap<String, String> {
    let mut pins = BTreeMap::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        let name = package["name"].as_str().unwrap_or("");
        if !LOCKSTEP_CRATES.contains(&name) {
            continue;
        }
        if let Some(sha) = package["source"].as_str().and_then(git_pin) {
            pins.insert(name.to_string(), sha.to_string());
        }
    }
    pins
}

/// Fail unless every lockstep crate pins the same rev in both lockfiles.
pub fn check_lockstep(
    workspace: &BTreeMap<String, String>,
    bench: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let mut drift = Vec::new();
    for name in LOCKSTEP_CRATES {
        match (workspace.get(*name), bench.get(*name)) {
            (Some(workspace_rev), Some(bench_rev)) if workspace_rev == bench_rev => {
                println!("{name}: {workspace_rev} (in sync)");
            }
            (Some(workspace_rev), Some(bench_rev)) => drift.push(format!(
                "{name} rev drift: workspace={workspace_rev} bench={bench_rev}"
            )),
            _ => drift.push(format!(
                "{name} rev missing: workspace={} bench={}",
                workspace.get(*name).map_or("none", String::as_str),
                bench.get(*name).map_or("none", String::as_str),
            )),
        }
    }
    if drift.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "lockstep pin drift — bump both manifests in one commit:\n{}",
            drift.join("\n")
        )
    }
}

/// Fail unless both manifests pin the same single rev per upstream repo —
/// the companion pins (`toasty-core`, `topcoat-ui*`) `bump-upstream` manages,
/// which the lock comparison never sees.
pub fn check_manifest_lockstep(
    workspace: &BTreeMap<String, BTreeSet<String>>,
    bench: &BTreeMap<String, BTreeSet<String>>,
) -> anyhow::Result<()> {
    let show = |revs: Option<&BTreeSet<String>>| {
        revs.map_or("none".to_string(), |set| {
            set.iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
    };
    let mut drift = Vec::new();
    for (name, _) in UPSTREAM_REPOS {
        match (workspace.get(*name), bench.get(*name)) {
            (Some(workspace_revs), Some(bench_revs))
                if workspace_revs == bench_revs && workspace_revs.len() == 1 =>
            {
                println!(
                    "{name} manifest pins: {} (in sync)",
                    show(Some(workspace_revs))
                );
            }
            (Some(_), Some(_)) => drift.push(format!(
                "{name} manifest rev drift: workspace={} bench={}",
                show(workspace.get(*name)),
                show(bench.get(*name)),
            )),
            _ => drift.push(format!(
                "{name} manifest rev missing: workspace={} bench={}",
                show(workspace.get(*name)),
                show(bench.get(*name)),
            )),
        }
    }
    if drift.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "manifest pin drift — bump both manifests in one commit:\n{}",
            drift.join("\n")
        )
    }
}

/// `cargo metadata` for one manifest. `--locked` (not `--frozen`): a stale
/// tree must fail instead of healing the lockfile, but fresh CI caches
/// legitimately need the network to resolve — `--offline` would mask the
/// drift signal with download errors.
fn fetch_metadata(manifest: &Path) -> anyhow::Result<serde_json::Value> {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .map_err(|error| anyhow::anyhow!("failed to run cargo metadata: {error}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "cargo metadata failed for {}: {}",
            manifest.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| anyhow::anyhow!("could not parse cargo metadata: {error}"))
}

/// Workspace vs bench rev equality, local and CI (the `bench-check` lockstep):
/// the `rev =` manifest pins `bump-upstream` manages, grouped by repo, then
/// the resolved lock pins. Manifests are plain file reads first — no
/// subprocess, no network, nothing to heal — so a hand-edited manifest
/// reports drift here instead of failing inside `cargo metadata --locked`
/// with a resolution error.
pub fn verify_locks() -> anyhow::Result<()> {
    let root = repo_root();
    let workspace_manifest = std::fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|error| anyhow::anyhow!("cannot read Cargo.toml: {error}"))?;
    let bench_manifest = std::fs::read_to_string(root.join("benchmarks/tablo/Cargo.toml"))
        .map_err(|error| anyhow::anyhow!("cannot read benchmarks/tablo/Cargo.toml: {error}"))?;
    check_manifest_lockstep(
        &manifest_revs(&workspace_manifest),
        &manifest_revs(&bench_manifest),
    )?;
    let workspace_meta = fetch_metadata(&root.join("Cargo.toml"))?;
    let bench_meta = fetch_metadata(&root.join("benchmarks/tablo/Cargo.toml"))?;
    check_lockstep(
        &pins_from_metadata(&workspace_meta),
        &pins_from_metadata(&bench_meta),
    )
}

fn is_rev(rev: &str) -> bool {
    rev.len() == 40 && rev.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The byte span of the `rev = "…"` value on one manifest line, if present.
fn rev_span(line: &str) -> Option<(usize, usize)> {
    let start = line.find("rev = \"")? + "rev = \"".len();
    let end = line[start..].find('"')? + start;
    Some((start, end))
}

/// The `rev = "…"` value on one manifest line, if present.
fn read_rev(line: &str) -> Option<&str> {
    let (start, end) = rev_span(line)?;
    Some(&line[start..end])
}

/// Replace the `rev = "…"` value on one manifest line.
fn replace_rev(line: &str, rev: &str) -> Option<String> {
    let (start, end) = rev_span(line)?;
    Some(format!("{}{rev}{}", &line[..start], &line[end..]))
}

/// The `rev =` pins in one manifest, grouped by repo short name: each repo
/// maps to its distinct pinned revs (one element when the manifest agrees
/// with itself). Reads through [`UPSTREAM_REPOS`], the same table
/// `set_upstream_revs` writes through.
pub fn manifest_revs(manifest: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut pins: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in manifest.split('\n') {
        let Some((name, _)) = UPSTREAM_REPOS.iter().find(|(_, url)| line.contains(url)) else {
            continue;
        };
        if let Some(rev) = read_rev(line) {
            pins.entry(name.to_string())
                .or_default()
                .insert(rev.to_string());
        }
    }
    pins
}

/// Rewrite the `rev =` pins for both upstream repos in one manifest.
/// Matches through [`UPSTREAM_REPOS`] so `toasty-core` and `topcoat-ui*`
/// follow their repo.
pub fn set_upstream_revs(
    manifest: &str,
    topcoat_rev: &str,
    toasty_rev: &str,
) -> anyhow::Result<String> {
    let revs = [topcoat_rev, toasty_rev];
    let mut hits = [0, 0];
    let mut out = Vec::new();
    for line in manifest.split('\n') {
        let mut rewritten = None;
        for (index, (_, url)) in UPSTREAM_REPOS.iter().enumerate() {
            if !line.contains(url) {
                continue;
            }
            if let Some(new_line) = replace_rev(line, revs[index]) {
                hits[index] += 1;
                rewritten = Some(new_line);
            }
            break;
        }
        out.push(rewritten.unwrap_or_else(|| line.to_string()));
    }
    if hits[0] == 0 || hits[1] == 0 {
        anyhow::bail!(
            "expected topcoat and toasty git pins, found {} and {}",
            hits[0],
            hits[1]
        );
    }
    Ok(out.join("\n"))
}

/// Bump both upstream revs in both manifests, re-resolve both lockfiles,
/// prove the revs resolve from the local git cache, and assert lockstep.
pub fn bump_upstream(run: &dyn Runner, topcoat_rev: &str, toasty_rev: &str) -> anyhow::Result<()> {
    for (name, rev) in [("topcoat", topcoat_rev), ("toasty", toasty_rev)] {
        if !is_rev(rev) {
            anyhow::bail!("{name} rev must be a 40-char hex sha, got `{rev}`");
        }
    }
    let root = repo_root();
    for manifest in ["Cargo.toml", "benchmarks/tablo/Cargo.toml"] {
        let path = root.join(manifest);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| anyhow::anyhow!("cannot read {}: {error}", path.display()))?;
        let updated = set_upstream_revs(&text, topcoat_rev, toasty_rev)?;
        std::fs::write(&path, updated)
            .map_err(|error| anyhow::anyhow!("cannot write {}: {error}", path.display()))?;
        println!("updated {manifest}");
    }
    run.run(
        "cargo",
        &["update", "-p", "topcoat", "-p", "toasty"],
        Some(&root),
        &[],
    )?;
    run.run(
        "cargo",
        &[
            "update",
            "--manifest-path",
            "benchmarks/tablo/Cargo.toml",
            "-p",
            "topcoat",
            "-p",
            "toasty",
        ],
        Some(&root),
        &[],
    )?;
    run.run("cargo", &["check", "--offline"], Some(&root), &[])?;
    run.run(
        "cargo",
        &[
            "check",
            "--offline",
            "--manifest-path",
            "benchmarks/tablo/Cargo.toml",
        ],
        Some(&root),
        &[],
    )?;
    verify_locks()
}

/// The gate set as a local fail-fast convenience runner: the eight CONTRIBUTING
/// gates in order, then docs, detached-bench fmt, and the lockstep check.
pub fn check(run: &dyn Runner) -> anyhow::Result<()> {
    check_with(run, &verify_locks)
}

fn check_with(run: &dyn Runner, verify: &dyn Fn() -> anyhow::Result<()>) -> anyhow::Result<()> {
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
    verify()
}

#[cfg(test)]
mod tests;
