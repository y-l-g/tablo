use std::cell::{Cell, RefCell};

use super::*;

type RecordedCommand = (String, Vec<String>, Option<PathBuf>, Vec<(String, String)>);

struct FakeRunner {
    commands: RefCell<Vec<RecordedCommand>>,
    fail_on: Option<usize>,
    /// What `probe` answers, as if the command had printed it and exited zero.
    probe_answer: Option<String>,
}

impl FakeRunner {
    fn ok() -> Self {
        Self {
            commands: RefCell::new(Vec::new()),
            fail_on: None,
            probe_answer: None,
        }
    }

    fn failing_on(index: usize) -> Self {
        Self {
            commands: RefCell::new(Vec::new()),
            fail_on: Some(index),
            probe_answer: None,
        }
    }

    /// A runner whose probe answers `answer`, e.g. an installed tool.
    fn answering(answer: &str) -> Self {
        Self {
            probe_answer: Some(answer.to_string()),
            ..Self::ok()
        }
    }

    fn commands(&self) -> Vec<RecordedCommand> {
        self.commands.borrow().clone()
    }
}

/// The recorded commands as `prog args`, for readable assertions.
fn lines(commands: &[RecordedCommand]) -> Vec<String> {
    commands
        .iter()
        .map(|(prog, args, _, _)| format!("{prog} {}", args.join(" ")))
        .collect()
}

impl Runner for FakeRunner {
    fn run(
        &self,
        prog: &str,
        args: &[&str],
        dir: Option<&Path>,
        env: &[(&str, &str)],
    ) -> anyhow::Result<()> {
        let index = self.commands.borrow().len();
        self.commands.borrow_mut().push((
            prog.to_string(),
            args.iter().map(ToString::to_string).collect(),
            dir.map(Path::to_path_buf),
            env.iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        ));
        if self.fail_on.is_some_and(|fail| fail == index) {
            anyhow::bail!("fake failure for {prog}");
        }
        Ok(())
    }

    fn probe(&self, _prog: &str, _args: &[&str]) -> Option<String> {
        self.probe_answer.clone()
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

fn workspace_manifest_fixture() -> &'static str {
    r#"topcoat = { git = "https://github.com/tokio-rs/topcoat", rev = "aaaa" }
toasty = { git = "https://github.com/tokio-rs/toasty", rev = "bbbb" }
toasty-core = { git = "https://github.com/tokio-rs/toasty", rev = "bbbb" }
topcoat-ui = { git = "https://github.com/tokio-rs/topcoat", rev = "aaaa" }
topcoat-ui-registry = { git = "https://github.com/tokio-rs/topcoat", rev = "aaaa" }
uuid = "1.23""#
}

fn bench_manifest_fixture() -> &'static str {
    r#"topcoat = { git = "https://github.com/tokio-rs/topcoat", rev = "aaaa", default-features = false }
toasty = { git = "https://github.com/tokio-rs/toasty", rev = "bbbb", default-features = false }
http = "1""#
}

#[test]
fn manifest_pins_group_companion_crates_by_repo() {
    let pins = manifest_revs(workspace_manifest_fixture());
    assert_eq!(
        pins,
        BTreeMap::from([
            ("topcoat".to_string(), BTreeSet::from(["aaaa".to_string()])),
            ("toasty".to_string(), BTreeSet::from(["bbbb".to_string()])),
        ])
    );
}

#[test]
fn manifest_lockstep_passes_when_revs_match() {
    let workspace = manifest_revs(workspace_manifest_fixture());
    let bench = manifest_revs(bench_manifest_fixture());
    check_manifest_lockstep(&workspace, &bench).expect("identical pins are in sync");
}

#[test]
fn manifest_lockstep_names_the_drifted_repo() {
    let workspace = manifest_revs(workspace_manifest_fixture());
    let mut bench = manifest_revs(bench_manifest_fixture());
    bench.insert("toasty".to_string(), BTreeSet::from(["cccc".to_string()]));
    let error = check_manifest_lockstep(&workspace, &bench).expect_err("drift must fail");
    assert!(
        error
            .to_string()
            .contains("toasty manifest rev drift: workspace=bbbb bench=cccc"),
        "unexpected message: {error}"
    );
}

#[test]
fn manifest_lockstep_fails_when_a_manifest_disagrees_with_itself() {
    let mut workspace = manifest_revs(workspace_manifest_fixture());
    workspace
        .get_mut("topcoat")
        .expect("topcoat pins")
        .insert("zzzz".to_string());
    let bench = manifest_revs(bench_manifest_fixture());
    let error = check_manifest_lockstep(&workspace, &bench).expect_err("self-drift must fail");
    assert!(
        error
            .to_string()
            .contains("topcoat manifest rev drift: workspace=aaaa, zzzz bench=aaaa"),
        "unexpected message: {error}"
    );
}

#[test]
fn manifest_lockstep_reports_a_missing_pin() {
    let workspace = manifest_revs(workspace_manifest_fixture());
    let bench = BTreeMap::from([("topcoat".to_string(), BTreeSet::from(["aaaa".to_string()]))]);
    let error = check_manifest_lockstep(&workspace, &bench).expect_err("a missing pin must fail");
    assert!(
        error.to_string().contains("toasty manifest rev missing"),
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
    let error = set_upstream_revs("uuid = \"1.23\"\n", "a", "b").expect_err("no pins must fail");
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
fn check_runs_gates_in_order_then_docs_fmt_and_lockstep() {
    let run = FakeRunner::ok();
    let verified = Cell::new(false);
    check_with(
        &run,
        CheckOptions::default(),
        &|| Ok(PathBuf::from("/staged/Cargo.toml")),
        &|| {
            verified.set(true);
            Ok(())
        },
    )
    .expect("fake commands succeed");
    assert!(verified.get(), "lockstep closes the run");
    let progs: Vec<String> = run
        .commands()
        .iter()
        .map(|(prog, _, _, _)| prog.clone())
        .collect();
    assert_eq!(
        progs,
        vec![
            "cargo", "cargo", "cargo", "topcoat", "git", "cargo", "cargo", "cargo", "cargo",
            "node", "cargo", "mdbook", "cargo", "cargo", "cargo", "cargo", "cargo",
        ]
    );
}

/// The default run covers every gate; `--quick` drops the ones needing another
/// toolchain or a build of a detached workspace.
#[test]
fn check_quick_leaves_out_the_toolchain_and_detached_gates() {
    let heavy = [
        "benchmarks/tablo",
        MSRV,
        "udeps",
        "doc --workspace",
        "mdbook",
    ];
    let run = FakeRunner::ok();
    check_with(
        &run,
        CheckOptions::default(),
        &|| Ok(PathBuf::from("/staged/Cargo.toml")),
        &|| Ok(()),
    )
    .expect("fake commands succeed");
    let full = lines(&run.commands());
    for gate in heavy {
        assert!(
            full.iter().any(|line| line.contains(gate)),
            "the default run covers `{gate}`: {full:?}"
        );
    }

    let run = FakeRunner::ok();
    check_with(
        &run,
        CheckOptions { quick: true },
        &|| Ok(PathBuf::from("/staged/Cargo.toml")),
        &|| Ok(()),
    )
    .expect("fake commands succeed");
    let quick = lines(&run.commands());
    for gate in heavy {
        assert!(
            !quick.iter().any(|line| line.contains(gate)),
            "`--quick` leaves out `{gate}`: {quick:?}"
        );
    }
    for gate in [
        "cargo test --workspace --locked",
        "cargo clippy --workspace --all-targets --locked -- -D warnings",
        "topcoat fmt",
    ] {
        assert!(
            quick.iter().any(|line| line == gate),
            "`--quick` keeps `{gate}`: {quick:?}"
        );
    }
    assert!(
        quick.iter().any(|line| line.starts_with("node --test ")),
        "`--quick` keeps the asset suites"
    );
}

/// The toolchain gates build into their own target directories: a 1.98 or
/// nightly build must not invalidate the stable artifacts in `target/`.
#[test]
fn check_isolates_the_toolchain_targets() {
    let run = FakeRunner::ok();
    check_with(
        &run,
        CheckOptions::default(),
        &|| Ok(PathBuf::from("/staged/Cargo.toml")),
        &|| Ok(()),
    )
    .expect("fake commands succeed");
    let commands = run.commands();
    let env_of = |subcommand: &str| {
        commands
            .iter()
            .find(|(_, args, _, _)| {
                args.first().is_some_and(|arg| arg.starts_with('+'))
                    && args.get(1).is_some_and(|arg| arg.as_str() == subcommand)
            })
            .unwrap_or_else(|| panic!("the {subcommand} gate runs"))
            .3
            .clone()
    };
    assert_eq!(
        env_of("check"),
        vec![("CARGO_TARGET_DIR".to_string(), MSRV_TARGET.to_string())]
    );
    assert_eq!(
        env_of("udeps"),
        vec![("CARGO_TARGET_DIR".to_string(), UDEPS_TARGET.to_string())]
    );
}

/// Gate 8 installs the pinned tool only when the probe does not answer with it.
#[test]
fn udeps_installs_the_pinned_tool_only_when_it_is_missing() {
    let pinned = format!("cargo-udeps {CARGO_UDEPS}");
    let stale = "cargo-udeps 0.1.50".to_string();
    for (answer, expected_install) in [(None, true), (Some(stale), true), (Some(pinned), false)] {
        let run = answer
            .as_deref()
            .map_or_else(FakeRunner::ok, FakeRunner::answering);
        check_with(
            &run,
            CheckOptions::default(),
            &|| Ok(PathBuf::from("/staged/Cargo.toml")),
            &|| Ok(()),
        )
        .expect("fake commands succeed");
        let installed = lines(&run.commands())
            .iter()
            .any(|line| line.contains("install cargo-udeps"));
        assert_eq!(
            installed, expected_install,
            "probe answered {answer:?}, install expected: {expected_install}"
        );
    }
}

#[test]
fn check_rejects_an_unknown_flag() {
    let error = CheckOptions::parse(&["--benchmark".to_string()])
        .expect_err("an unknown flag must fail before any gate runs");
    assert!(
        error
            .to_string()
            .contains("unknown `check` flag: --benchmark"),
        "unexpected message: {error}"
    );
}

#[test]
fn check_stops_at_the_first_failure() {
    let run = FakeRunner::failing_on(1);
    check_with(
        &run,
        CheckOptions::default(),
        &|| panic!("staging must not run after a gate fails"),
        &|| panic!("lockstep must not run after a gate fails"),
    )
    .expect_err("a failing gate must fail the run");
    assert_eq!(run.commands().len(), 2, "fail-fast stops after the failure");
}

#[test]
fn bump_rejects_a_non_sha_rev_before_touching_anything() {
    let run = FakeRunner::ok();
    let error =
        bump_upstream(&run, "not-a-sha", "also-not-a-sha").expect_err("a non-sha rev must fail");
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
    let ci = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read ci.yml");
    let msrv_udeps = std::fs::read_to_string(root.join(".github/workflows/msrv-udeps.yml"))
        .expect("read msrv-udeps.yml");
    let contributing =
        std::fs::read_to_string(root.join("CONTRIBUTING.md")).expect("read CONTRIBUTING.md");
    let skill = std::fs::read_to_string(root.join(".agents/skills/check/SKILL.md"))
        .expect("read the check skill");
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("read Cargo.toml");
    assert!(ci.contains(NIGHTLY_FMT), "ci.yml names the nightly");
    assert!(
        msrv_udeps.contains(NIGHTLY_UDEPS),
        "msrv-udeps.yml names the udeps nightly"
    );
    assert!(
        msrv_udeps.contains(CARGO_UDEPS),
        "msrv-udeps.yml names the pinned cargo-udeps"
    );
    assert!(
        msrv_udeps.contains(MSRV),
        "msrv-udeps.yml names the MSRV floor"
    );
    assert!(
        contributing.contains(NIGHTLY_FMT),
        "CONTRIBUTING.md names the nightly"
    );
    assert!(
        skill.contains(NIGHTLY_UDEPS),
        "the check skill names the udeps nightly"
    );
    assert!(
        skill.contains(CARGO_UDEPS),
        "the check skill names the pinned cargo-udeps"
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
fn the_quickstart_manifest_pins_the_workspace_revs() {
    let root = repo_root();
    let read = |path: &str| {
        std::fs::read_to_string(root.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
    };
    check_manifest_pins(
        &manifest_revs(&read("Cargo.toml")),
        &manifest_revs(&read("examples/quickstart/Cargo.toml")),
        "quickstart",
    )
    .expect("the quickstart pins the workspace's upstream revs");
    absolute_crate_paths(&read("examples/quickstart/Cargo.toml"), &root)
        .expect("the quickstart names the crates by relative path");
}

#[test]
fn external_stage_is_stable_per_checkout_and_distinct_across_them() {
    let one = external_stage(Path::new("/repo/one"));
    assert_eq!(one, external_stage(Path::new("/repo/one")));
    assert_ne!(one, external_stage(Path::new("/repo/.worktrees/two")));
    assert!(one.starts_with(std::env::temp_dir()));
}
