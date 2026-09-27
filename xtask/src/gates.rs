//! Gate orchestration: `fmt`, `bump-upstream`, `verify-locks`, `check`.
//!
//! Each CI job calls one subcommand instead of inlining its own bash loop, so
//! the loops are typed, tested, and runnable locally. Jobs stay parallel —
//! `check` is a local fail-fast convenience runner only, never a CI job.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

/// Crates pinned in lockstep across the workspace and bench lockfiles (GH #103).
pub const LOCKSTEP_CRATES: &[&str] = &["topcoat", "toasty"];

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

/// The pinned-nightly workspace fmt check (CONTRIBUTING gate 4).
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

/// The locked-rev `topcoat fmt` check plus diff guard (CONTRIBUTING gate 5).
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

fn fetch_metadata(manifest: &Path) -> anyhow::Result<serde_json::Value> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--manifest-path"])
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

/// Workspace vs bench rev equality, local and CI (the `bench-check` lockstep).
pub fn verify_locks() -> anyhow::Result<()> {
    let root = repo_root();
    let workspace = fetch_metadata(&root.join("Cargo.toml"))?;
    let bench = fetch_metadata(&root.join("benchmarks/tablo/Cargo.toml"))?;
    check_lockstep(&pins_from_metadata(&workspace), &pins_from_metadata(&bench))
}

fn is_rev(rev: &str) -> bool {
    rev.len() == 40 && rev.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Replace the `rev = "…"` value on one manifest line.
fn replace_rev(line: &str, rev: &str) -> Option<String> {
    let start = line.find("rev = \"")? + "rev = \"".len();
    let end = line[start..].find('"')? + start;
    Some(format!("{}{rev}{}", &line[..start], &line[end..]))
}

/// Rewrite the `rev =` pins for both upstream repos in one manifest.
/// Matches on the git URL so `toasty-core` and `topcoat-ui*` follow their repo.
pub fn set_upstream_revs(
    manifest: &str,
    topcoat_rev: &str,
    toasty_rev: &str,
) -> anyhow::Result<String> {
    let mut topcoat_hits = 0;
    let mut toasty_hits = 0;
    let mut out = Vec::new();
    for line in manifest.split('\n') {
        if line.contains("github.com/tokio-rs/topcoat") {
            match replace_rev(line, topcoat_rev) {
                Some(rewritten) => {
                    topcoat_hits += 1;
                    out.push(rewritten);
                }
                None => out.push(line.to_string()),
            }
        } else if line.contains("github.com/tokio-rs/toasty") {
            match replace_rev(line, toasty_rev) {
                Some(rewritten) => {
                    toasty_hits += 1;
                    out.push(rewritten);
                }
                None => out.push(line.to_string()),
            }
        } else {
            out.push(line.to_string());
        }
    }
    if topcoat_hits == 0 || toasty_hits == 0 {
        anyhow::bail!(
            "expected topcoat and toasty git pins, found {topcoat_hits} and {toasty_hits}"
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

/// The gate set as a local fail-fast convenience runner: the ten CONTRIBUTING
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
    run.run(
        "cargo",
        &[
            "test",
            "-p",
            "tablo-core",
            "--no-default-features",
            "--locked",
        ],
        Some(&root),
        &[],
    )?;
    nightly_fmt(run, &root)?;
    topcoat_fmt(run, &root)?;
    run.run(
        "cargo",
        &[
            "check",
            "--locked",
            "--manifest-path",
            "benchmarks/tablo/Cargo.toml",
        ],
        Some(&root),
        &[],
    )?;
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
            "--all-features",
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
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    type RecordedCommand = (String, Vec<String>, Option<PathBuf>);

    struct FakeRunner {
        commands: RefCell<Vec<RecordedCommand>>,
        fail_on: Option<usize>,
    }

    impl FakeRunner {
        fn ok() -> Self {
            Self {
                commands: RefCell::new(Vec::new()),
                fail_on: None,
            }
        }

        fn failing_on(index: usize) -> Self {
            Self {
                commands: RefCell::new(Vec::new()),
                fail_on: Some(index),
            }
        }

        fn commands(&self) -> Vec<RecordedCommand> {
            self.commands.borrow().clone()
        }
    }

    impl Runner for FakeRunner {
        fn run(
            &self,
            prog: &str,
            args: &[&str],
            dir: Option<&Path>,
            _env: &[(&str, &str)],
        ) -> anyhow::Result<()> {
            let index = self.commands.borrow().len();
            self.commands.borrow_mut().push((
                prog.to_string(),
                args.iter().map(ToString::to_string).collect(),
                dir.map(Path::to_path_buf),
            ));
            if self.fail_on.is_some_and(|fail| fail == index) {
                anyhow::bail!("fake failure for {prog}");
            }
            Ok(())
        }
    }

    fn metadata_fixture() -> serde_json::Value {
        serde_json::from_str(
            r#"{"packages": [
                {"name": "topcoat", "source": "git+https://github.com/tokio-rs/topcoat?rev=aaaa#aaaa"},
                {"name": "toasty", "source": "git+https://github.com/tokio-rs/toasty?rev=bbbb#bbbb"},
                {"name": "serde", "source": "registry+https://github.com/rust-lang/crates.io-index"},
                {"name": "tablo-core"}
            ]}"#,
        )
        .expect("fixture parses")
    }

    #[test]
    fn pins_parse_git_shas_from_metadata() {
        let pins = pins_from_metadata(&metadata_fixture());
        assert_eq!(
            pins,
            BTreeMap::from([
                ("topcoat".to_string(), "aaaa".to_string()),
                ("toasty".to_string(), "bbbb".to_string()),
            ])
        );
    }

    #[test]
    fn lockstep_passes_when_revs_match() {
        let pins = pins_from_metadata(&metadata_fixture());
        check_lockstep(&pins, &pins).expect("identical pins are in sync");
    }

    #[test]
    fn lockstep_names_the_drifted_crate() {
        let workspace = pins_from_metadata(&metadata_fixture());
        let mut bench = workspace.clone();
        bench.insert("toasty".to_string(), "cccc".to_string());
        let error = check_lockstep(&workspace, &bench).expect_err("drift must fail");
        assert!(
            error
                .to_string()
                .contains("toasty rev drift: workspace=bbbb bench=cccc"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn lockstep_reports_a_missing_pin() {
        let workspace = pins_from_metadata(&metadata_fixture());
        let bench = BTreeMap::from([("topcoat".to_string(), "aaaa".to_string())]);
        let error = check_lockstep(&workspace, &bench).expect_err("a missing pin must fail");
        assert!(
            error.to_string().contains("toasty rev missing"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn set_upstream_revs_rewrites_every_pin_of_both_repos() {
        let manifest = r#"topcoat = { git = "https://github.com/tokio-rs/topcoat", rev = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }
toasty = { git = "https://github.com/tokio-rs/toasty", rev = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" }
toasty-core = { git = "https://github.com/tokio-rs/toasty", rev = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" }
topcoat-ui = { git = "https://github.com/tokio-rs/topcoat", rev = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }
uuid = "1.23""#;
        let updated = set_upstream_revs(
            manifest,
            "cccccccccccccccccccccccccccccccccccccccc",
            "dddddddddddddddddddddddddddddddddddddddd",
        )
        .expect("pins rewrite");
        assert_eq!(
            updated
                .matches("cccccccccccccccccccccccccccccccccccccccc")
                .count(),
            2
        );
        assert_eq!(
            updated
                .matches("dddddddddddddddddddddddddddddddddddddddd")
                .count(),
            2
        );
        assert!(
            updated.contains("uuid = \"1.23\""),
            "unrelated lines survive"
        );
        assert!(!updated.contains('a'.to_string().repeat(40).as_str()));
    }

    #[test]
    fn set_upstream_revs_rejects_a_manifest_without_pins() {
        let error =
            set_upstream_revs("uuid = \"1.23\"\n", "a", "b").expect_err("no pins must fail");
        assert!(
            error
                .to_string()
                .contains("expected topcoat and toasty git pins")
        );
    }

    #[test]
    fn fmt_runs_nightly_detached_and_topcoat_checks() {
        let run = FakeRunner::ok();
        fmt_check(&run).expect("fake commands succeed");
        let root = repo_root();
        let commands = run.commands();
        assert_eq!(commands.len(), 6, "1 nightly + 3 detached + topcoat + diff");
        assert_eq!(
            commands[0].1,
            vec!["+nightly-2026-08-24", "fmt", "--all", "--", "--check"]
        );
        for (command, bench) in commands[1..4].iter().zip(DETACHED_BENCHES) {
            assert_eq!(command.0, "cargo");
            assert_eq!(command.1, vec!["fmt", "--", "--check"]);
            assert_eq!(command.2, Some(root.join(bench)));
        }
        assert_eq!(commands[4].0, "topcoat");
        assert_eq!(commands[4].1, vec!["fmt"]);
        assert_eq!(commands[5].0, "git");
        assert_eq!(commands[5].1, vec!["diff", "--exit-code"]);
        assert!(commands.iter().all(|command| {
            command
                .2
                .as_ref()
                .is_some_and(|dir| dir == &root || dir.starts_with(&root))
        }));
    }

    #[test]
    fn check_runs_gates_in_order_then_docs_fmt_and_lockstep() {
        let run = FakeRunner::ok();
        let verified = Cell::new(false);
        check_with(&run, &|| {
            verified.set(true);
            Ok(())
        })
        .expect("fake commands succeed");
        assert!(verified.get(), "lockstep closes the run");
        let progs: Vec<String> = run
            .commands()
            .iter()
            .map(|(prog, _, _)| prog.clone())
            .collect();
        assert_eq!(
            progs,
            vec![
                "cargo", "cargo", "cargo", "cargo", "topcoat", "git", "cargo", "cargo", "cargo",
                "node", "cargo", "cargo", "cargo", "mdbook", "cargo", "cargo", "cargo",
            ]
        );
    }

    #[test]
    fn check_stops_at_the_first_failure() {
        let run = FakeRunner::failing_on(1);
        check_with(&run, &|| {
            panic!("lockstep must not run after a gate fails");
        })
        .expect_err("a failing gate must fail the run");
        assert_eq!(run.commands().len(), 2, "fail-fast stops after the failure");
    }

    #[test]
    fn bump_rejects_a_non_sha_rev_before_touching_anything() {
        let run = FakeRunner::ok();
        let error = bump_upstream(&run, "not-a-sha", "also-not-a-sha")
            .expect_err("a non-sha rev must fail");
        assert!(error.to_string().contains("must be a 40-char hex sha"));
        assert!(
            run.commands().is_empty(),
            "no command runs before validation"
        );
    }

    /// The pins xtask shells out with must stay the ones CI and the docs name.
    #[test]
    fn pins_match_ci_and_docs() {
        let root = repo_root();
        let ci =
            std::fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci.yml");
        let contributing =
            std::fs::read_to_string(root.join("CONTRIBUTING.md")).expect("read CONTRIBUTING.md");
        let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("read Cargo.toml");
        assert!(ci.contains(NIGHTLY_FMT), "ci.yml names the nightly");
        assert!(
            contributing.contains(NIGHTLY_FMT),
            "CONTRIBUTING.md names the nightly"
        );
        for bench in DETACHED_BENCHES {
            assert!(ci.contains(bench), "ci.yml covers {bench}");
        }
        for suite in ASSET_SUITES {
            assert!(ci.contains(suite), "ci.yml names {suite}");
        }
        assert!(
            manifest.contains(&format!("rust-version = \"{MSRV}\"")),
            "Cargo.toml carries the MSRV floor"
        );
    }
}
