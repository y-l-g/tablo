use xtask::gates::RealRunner;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "help".to_string());
    match cmd.as_str() {
        "sync-topcoat-ui" | "sync" => {
            // One collected list: reading two flags from the same `Iterator`
            // loses the later one when the earlier scan has already consumed
            // it (`--prune` alone, or after `--dry-run`).
            let rest: Vec<String> = args.collect();
            let dry_run = rest.iter().any(|a| a == "--dry-run");
            let prune = rest.iter().any(|a| a == "--prune");
            xtask::sync_topcoat_ui(dry_run, prune)?;
        }
        "verify-topcoat-ui" | "verify" => {
            xtask::verify_sync()?;
        }
        "fmt" => {
            xtask::gates::fmt_check(&RealRunner)?;
        }
        "verify-locks" => {
            xtask::gates::verify_locks()?;
        }
        "check" => {
            xtask::gates::check(&RealRunner)?;
        }
        "bump-upstream" => {
            let rest: Vec<String> = args.collect();
            if rest.len() != 2 {
                eprintln!("usage: cargo xtask bump-upstream <TOPCOAT_REV> <TOASTY_REV>");
                std::process::exit(1);
            }
            xtask::gates::bump_upstream(&RealRunner, &rest[0], &rest[1])?;
        }
        "--help" | "-h" | "help" => {
            print_help();
        }
        other => {
            eprintln!("unknown command: {other}");
            print_help();
            std::process::exit(1);
        }
    }
    Ok(())
}

fn print_help() {
    println!(
        r#"xtask — repo tasks (ADR-0007)

USAGE:
    cargo xtask sync-topcoat-ui [--dry-run] [--prune]
    cargo xtask verify-topcoat-ui
    cargo xtask fmt
    cargo xtask verify-locks
    cargo xtask check
    cargo xtask bump-upstream <TOPCOAT_REV> <TOASTY_REV>

COMMANDS:
    sync-topcoat-ui    Copy the components in `xtask::VENDORED_PRIMITIVES`
                       from the `topcoat-ui-registry` crate Cargo resolved
                       for this workspace into
                       crates/tablo-ui/src/components/primitives/*.rs —
                       verbatim, under a SYNC header recording the registry
                       version and the source's sha256 content hash. Never
                       touches composites/. No sibling clone required — the
                       registry comes from the same git source Cargo
                       compiles against.
    verify-topcoat-ui  Guard: fail when any vendored primitive (or mod.rs)
                       has drifted from the registry. The xtask test suite
                       runs this on every `cargo test`.
    fmt                The formatting subset: nightly `cargo fmt --check`,
                       detached-bench `cargo fmt --check`, and the locked-rev
                       `topcoat fmt` plus diff guard. Check-half only: it
                       never installs the topcoat CLI, and fails with the
                       locked-rev install command when the CLI is missing
                       or the wrong rev.
    verify-locks       Guard: fail when `Cargo.lock` and
                       `benchmarks/tablo/Cargo.lock` pin different `topcoat`
                       or `toasty` revs, or when the `rev =` manifest pins
                       disagree. The xtask test suite runs this on
                       every `cargo test`.
    check              The gate set as a local fail-fast convenience runner:
                       the eight CONTRIBUTING gates in order, then docs,
                       detached-bench fmt, and the lockstep check. CI keeps
                       one subcommand per parallel job instead.
    bump-upstream      Rewrite the `rev =` pins for both upstream repos in
                       both manifests, re-resolve both lockfiles, prove the
                       revs resolve from the local git cache
                       (`cargo check --offline`), and assert lockstep.

OPTIONS:
    --dry-run          Print what would be copied without writing
    --prune            Also delete vendored files the vendored set no longer owns
    --help, -h         Show this help
"#
    );
}
