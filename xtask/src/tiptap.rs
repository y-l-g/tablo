//! The vendored TipTap bundle (ADR-0031): `vendor-tiptap` builds `assets/tiptap/tiptap.js` from
//! the lockfile beside it, and the guard checks the bundle names that lockfile's hash.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use sha2::{Digest, Sha256};

use crate::gates::Runner;

/// The bundle's first line, naming the lockfile it was built from.
pub fn header(lock: &[u8]) -> String {
    let hash: String = Sha256::digest(lock)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!(
        "// Built by `cargo xtask vendor-tiptap` from package-lock.json sha256:{hash}; do not edit."
    )
}

/// The directory holding the bundle's `package.json`, lockfile, entry and output.
pub fn tiptap_dir() -> PathBuf {
    crate::assets_dir().join("tiptap")
}

/// Installs the locked packages in a scratch directory and bundles `entry.js` into one minified
/// ES module, `tiptap.js`, headed by [`header`]. Needs `npm` and the npm registry.
pub fn vendor_tiptap(run: &dyn Runner) -> anyhow::Result<()> {
    let dir = tiptap_dir();
    let lock = std::fs::read(dir.join("package-lock.json")).context("read package-lock.json")?;
    let scratch = std::env::temp_dir().join(format!("tablo-tiptap-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    for file in ["package.json", "package-lock.json", "entry.js"] {
        std::fs::copy(dir.join(file), scratch.join(file))
            .with_context(|| format!("copy {file}"))?;
    }
    let built = bundle(run, &scratch, &dir, &header(&lock));
    std::fs::remove_dir_all(&scratch)?;
    built?;
    println!("wrote {}", dir.join("tiptap.js").display());
    Ok(())
}

fn bundle(run: &dyn Runner, scratch: &Path, dir: &Path, header: &str) -> anyhow::Result<()> {
    // `--ignore-scripts`: no package's install script runs; esbuild finds its binary through
    // its optional platform package without one.
    run.run(
        "npm",
        &["ci", "--ignore-scripts", "--no-audit", "--no-fund"],
        Some(scratch),
        &[],
    )?;
    let outfile = format!("--outfile={}", dir.join("tiptap.js").display());
    let banner = format!("--banner:js={header}");
    run.run(
        "node_modules/.bin/esbuild",
        &[
            "entry.js",
            "--bundle",
            "--format=esm",
            "--minify",
            "--target=es2022",
            "--legal-comments=eof",
            &banner,
            &outfile,
        ],
        Some(scratch),
        &[],
    )
}

/// Fails when `tiptap.js` was not built from the lockfile beside it: a bumped package without a
/// rebuild, or a hand edit of the header.
pub fn verify_tiptap() -> anyhow::Result<()> {
    let dir = tiptap_dir();
    let lock = std::fs::read(dir.join("package-lock.json")).context("read package-lock.json")?;
    let bundle = std::fs::read_to_string(dir.join("tiptap.js")).context("read tiptap.js")?;
    let first = bundle.lines().next().unwrap_or_default();
    anyhow::ensure!(
        first == header(&lock),
        "tiptap.js was not built from package-lock.json; run `cargo xtask vendor-tiptap`"
    );
    Ok(())
}
