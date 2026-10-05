use std::cell::RefCell;

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

#[test]
fn fmt_runs_nightly_detached_and_topcoat_checks() {
    let run = FakeRunner::ok();
    fmt_check(&run).expect("fake commands succeed");
    let root = repo_root();
    let commands = run.commands();
    assert_eq!(
        commands.len(),
        7,
        "1 nightly + 3 detached benches + quickstart + topcoat + diff"
    );
    assert_eq!(
        commands[0].1,
        vec!["+nightly-2026-08-24", "fmt", "--all", "--", "--check"]
    );
    for (command, dir) in commands[1..5]
        .iter()
        .zip(DETACHED_BENCHES.iter().chain([&QUICKSTART]))
    {
        assert_eq!(command.0, "cargo");
        assert_eq!(command.1, vec!["fmt", "--", "--check"]);
        assert_eq!(command.2, Some(root.join(dir)));
    }
    assert_eq!(commands[5].0, "topcoat");
    assert_eq!(commands[5].1, vec!["fmt"]);
    assert_eq!(commands[6].0, "git");
    assert_eq!(commands[6].1, vec!["diff", "--exit-code"]);
    assert!(commands.iter().all(|command| {
        command
            .2
            .as_ref()
            .is_some_and(|dir| dir == &root || dir.starts_with(&root))
    }));
}

fn run_check(scope: Scope, touch: bool) -> Vec<RecordedCommand> {
    let run = FakeRunner::ok();
    check_with(
        &run,
        &|| Ok(PathBuf::from("/staged/Cargo.toml")),
        scope,
        &|_| touch,
    )
    .expect("fake commands succeed");
    run.commands()
}

fn progs_of(commands: &[RecordedCommand]) -> Vec<String> {
    commands.iter().map(|(prog, _, _)| prog.clone()).collect()
}

#[test]
fn check_runs_cheap_gates_first_then_builds_docs_and_external() {
    assert_eq!(
        progs_of(&run_check(Scope::Auto, true)),
        vec![
            "cargo", "topcoat", "git", "node", "cargo", "cargo", "cargo", "cargo", "cargo",
            "cargo", "cargo", "cargo", "cargo", "cargo", "cargo", "mdbook", "cargo",
        ]
    );
}

#[test]
fn check_runs_skipped_gates_only_with_matching_changes_or_all() {
    assert_eq!(
        progs_of(&run_check(Scope::Auto, false)),
        vec![
            "cargo", "topcoat", "git", "node", "cargo", "cargo", "cargo", "cargo", "cargo",
            "cargo", "cargo", "mdbook", "cargo",
        ],
        "bench, msrv, and udeps install+run are skipped"
    );
    assert_eq!(
        run_check(Scope::All, false).len(),
        17,
        "`--all` forces every gate"
    );
}

#[test]
fn check_stops_at_the_first_failure() {
    let run = FakeRunner::failing_on(1);
    check_with(
        &run,
        &|| panic!("staging must not run after a gate fails"),
        Scope::Auto,
        &|_| true,
    )
    .expect_err("a failing gate must fail the run");
    assert_eq!(run.commands().len(), 2, "fail-fast stops after the failure");
}

/// The pins xtask shells out with must stay the ones CI and the docs name.
#[test]
fn pins_match_ci_and_docs() {
    let root = repo_root();
    let ci = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci.yml");
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

/// Reads a workflow's `pull_request: paths` list: the `- entry` lines under `paths:`.
fn workflow_paths(workflow: &str) -> Vec<String> {
    let mut paths: Vec<String> = workflow
        .lines()
        .skip_while(|line| line.trim() != "paths:")
        .skip(1)
        .map(str::trim)
        .take_while(|line| line.starts_with("- "))
        // YAML quotes globs (`"**/Cargo.toml"`): compare the path itself.
        .map(|line| line[2..].trim_matches('"').to_string())
        .collect();
    paths.sort();
    paths
}

/// The local skip filters must mirror the CI path filters exactly: a local
/// skip matches a CI skip, so `check` cannot pass what CI runs.
#[test]
fn skip_filters_match_ci_workflows() {
    let root = repo_root();
    let bench =
        std::fs::read_to_string(root.join(".github/workflows/bench.yml")).expect("read bench.yml");
    let msrv = std::fs::read_to_string(root.join(".github/workflows/msrv-udeps.yml"))
        .expect("read msrv-udeps.yml");
    let mut bench_paths: Vec<String> = BENCH_PATHS.iter().map(ToString::to_string).collect();
    bench_paths.sort();
    assert_eq!(
        workflow_paths(&bench),
        bench_paths,
        "BENCH_PATHS mirrors bench.yml"
    );
    let mut msrv_paths: Vec<String> = MSRV_PATHS.iter().map(ToString::to_string).collect();
    msrv_paths.sort();
    assert_eq!(
        workflow_paths(&msrv),
        msrv_paths,
        "MSRV_PATHS mirrors msrv-udeps.yml"
    );
}

fn git_in(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

fn scratch_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("clear the scratch repo");
    }
    std::fs::create_dir_all(&dir).expect("create the scratch repo");
    git_in(&dir, &["init", "-b", "master", "-q"]);
    git_in(&dir, &["config", "user.email", "xtask@test"]);
    git_in(&dir, &["config", "user.name", "xtask"]);
    dir
}

/// The skip detector reads the committed branch diff and the uncommitted
/// worktree state through git's own pathspec matching, and runs the gate when
/// git cannot answer.
#[test]
fn changes_touch_reads_committed_and_uncommitted_state() {
    let outside = std::env::temp_dir().join("tablo-gates-no-repo");
    assert!(
        changes_touch(&outside, MSRV_PATHS),
        "a gate that cannot prove irrelevance runs"
    );
    let dir = scratch_repo("tablo-gates-touch");
    std::fs::write(dir.join("notes.md"), "seed\n").expect("write the seed file");
    git_in(&dir, &["add", "."]);
    git_in(&dir, &["commit", "-qm", "seed"]);
    assert!(
        !changes_touch(&dir, MSRV_PATHS),
        "a clean tree at master touches nothing"
    );
    std::fs::write(dir.join("notes.md"), "edit\n").expect("edit a tracked file");
    assert!(
        !changes_touch(&dir, MSRV_PATHS),
        "an edit outside the filter still skips"
    );
    git_in(&dir, &["checkout", "-qb", "work"]);
    std::fs::write(dir.join("Cargo.toml"), "[package]\n").expect("add a manifest");
    git_in(&dir, &["add", "."]);
    git_in(&dir, &["commit", "-qm", "manifest"]);
    assert!(
        changes_touch(&dir, MSRV_PATHS),
        "a committed manifest on the branch runs the gate"
    );
    git_in(&dir, &["checkout", "-q", "master"]);
    git_in(&dir, &["checkout", "-qb", "bench-only", "master"]);
    std::fs::create_dir_all(dir.join("benchmarks/tablo")).expect("create the bench dir");
    std::fs::write(dir.join("benchmarks/tablo/main.rs"), "fn main() {}\n")
        .expect("add a bench file");
    git_in(&dir, &["add", "."]);
    git_in(&dir, &["commit", "-qm", "bench"]);
    assert!(
        changes_touch(&dir, BENCH_PATHS),
        "a committed bench change runs the bench gate"
    );
    assert!(
        !changes_touch(&dir, MSRV_PATHS),
        "a bench-only change still skips MSRV/udeps"
    );
    std::fs::remove_dir_all(&dir).expect("remove the scratch repo");
}

#[test]
fn absolute_crate_paths_points_the_quickstart_at_the_repository() {
    let manifest = "tablo = { path = \"../../crates/tablo\" }\n\
                    tablo-build = { path = \"../../crates/tablo-build\" }\n";
    let rewritten =
        absolute_crate_paths(manifest, Path::new("/repo")).expect("the manifest has paths");
    assert_eq!(
        rewritten,
        "tablo = { path = \"/repo/crates/tablo\" }\n\
         tablo-build = { path = \"/repo/crates/tablo-build\" }\n"
    );
}

#[test]
fn absolute_crate_paths_refuses_a_manifest_without_crate_paths() {
    let error = absolute_crate_paths("tablo = \"0.1\"\n", Path::new("/repo"))
        .expect_err("nothing to rewrite must fail");
    assert!(
        error.to_string().contains("names no"),
        "unexpected message: {error}"
    );
}

#[test]
fn external_stage_is_stable_per_checkout_and_distinct_across_them() {
    let one = external_stage(Path::new("/repo/one"));
    assert_eq!(one, external_stage(Path::new("/repo/one")));
    assert_ne!(one, external_stage(Path::new("/repo/.worktrees/two")));
    assert!(one.starts_with(std::env::temp_dir()));
}
