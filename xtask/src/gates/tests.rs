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

#[test]
fn check_runs_gates_in_order_then_docs_and_fmt() {
    let run = FakeRunner::ok();
    check_with(&run, &|| Ok(PathBuf::from("/staged/Cargo.toml"))).expect("fake commands succeed");
    let progs: Vec<String> = run
        .commands()
        .iter()
        .map(|(prog, _, _)| prog.clone())
        .collect();
    assert_eq!(
        progs,
        vec![
            "cargo", "cargo", "cargo", "topcoat", "git", "cargo", "cargo", "node", "cargo",
            "cargo", "cargo", "mdbook", "cargo", "cargo", "cargo", "cargo", "cargo",
        ]
    );
}

#[test]
fn check_stops_at_the_first_failure() {
    let run = FakeRunner::failing_on(1);
    check_with(&run, &|| panic!("staging must not run after a gate fails"))
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
