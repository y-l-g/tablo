use xtask::gates::{RealRunner, Scope};

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
        "external-check" => {
            let root = xtask::gates::repo_root();
            let manifest = xtask::gates::stage_quickstart(&root)?;
            xtask::gates::external_check(&RealRunner, &root, &manifest)?;
        }
        "check" => {
            let mut all = false;
            for arg in args {
                if arg == "--all" {
                    all = true;
                } else {
                    anyhow::bail!("unknown flag for check: {arg} (only --all)");
                }
            }
            xtask::gates::check(&RealRunner, if all { Scope::All } else { Scope::Auto })?;
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
        r#"xtask — repo tasks

USAGE:
    cargo xtask check [--all]      The CI checks, fail-fast and cheapest first. MSRV and udeps
                                   run when a manifest changed; --all adds them plus rustdoc,
                                   the guide and the external build.
    cargo xtask fmt                Nightly `cargo fmt`, the quickstart's `cargo fmt`, and the
                                   pinned `topcoat fmt` with a diff guard.
    cargo xtask external-check     Build and test examples/quickstart from outside the repo.
    cargo xtask sync-topcoat-ui [--dry-run] [--prune]
                                   Copy the vendored primitives verbatim from the registry
                                   (ADR-0007); --prune deletes files no longer vendored.
    cargo xtask verify-topcoat-ui  Fail when a vendored primitive drifted from the registry.
"#
    );
}
