//! The vendored TipTap bundle (ADR-0032): `vendor-tiptap` builds `assets/tiptap/tiptap.js` and the
//! licences of the packages it holds from the files beside it, and the guard checks the bundle
//! names the hashes of what it was built from and of what the build wrote.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use sha2::{Digest, Sha256};

use crate::gates::Runner;

/// The files the build reads, beside the bundle.
const INPUTS: [&str; 3] = ["package.json", "package-lock.json", "entry.js"];

/// The esbuild flags, past the entry and the output paths.
const ESBUILD: [&str; 5] = [
    "--bundle",
    "--format=esm",
    "--minify",
    "--target=es2022",
    "--legal-comments=none",
];

/// The licences of the packages the bundle holds, beside it.
const LICENSES: &str = "THIRD_PARTY_LICENSES.txt";

fn sha256(parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The hash of the build's inputs in `dir` and of its flags.
fn inputs_hash(dir: &Path) -> anyhow::Result<String> {
    let mut parts = Vec::new();
    for file in INPUTS {
        parts.push(std::fs::read(dir.join(file)).with_context(|| format!("read {file}"))?);
    }
    parts.push(ESBUILD.join(" ").into_bytes());
    Ok(sha256(&parts.iter().map(Vec::as_slice).collect::<Vec<_>>()))
}

/// The bundle's first line, naming the hash of the build's inputs and of the `body` and
/// `licenses` it wrote.
pub fn header(inputs: &str, body: &str, licenses: &str) -> String {
    let output = sha256(&[body.as_bytes(), licenses.as_bytes()]);
    format!(
        "// Built by `cargo xtask vendor-tiptap` from inputs sha256:{inputs} into output \
         sha256:{output}; do not edit."
    )
}

/// The directory holding the bundle's `package.json`, lockfile, entry and output.
pub fn tiptap_dir() -> PathBuf {
    crate::assets_dir().join("tiptap")
}

/// Installs the locked packages in a scratch directory, bundles `entry.js` into one minified ES
/// module, `tiptap.js`, headed by [`header`], and gathers the licences of the packages it holds.
/// Needs `npm` and the npm registry.
pub fn vendor_tiptap(run: &dyn Runner) -> anyhow::Result<()> {
    let dir = tiptap_dir();
    let scratch = std::env::temp_dir().join(format!("tablo-tiptap-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)?;
    for file in INPUTS {
        std::fs::copy(dir.join(file), scratch.join(file))
            .with_context(|| format!("copy {file}"))?;
    }
    let built = bundle(run, &scratch);
    std::fs::remove_dir_all(&scratch)?;
    let (body, licenses) = built?;
    let header = header(&inputs_hash(&dir)?, &body, &licenses);
    std::fs::write(dir.join("tiptap.js"), format!("{header}\n{body}"))?;
    std::fs::write(dir.join(LICENSES), licenses)?;
    println!("wrote {}", dir.join("tiptap.js").display());
    Ok(())
}

/// Builds the bundle in `scratch`: its body and the licences of the packages it holds.
fn bundle(run: &dyn Runner, scratch: &Path) -> anyhow::Result<(String, String)> {
    // `--ignore-scripts`: no package's install script runs; esbuild finds its binary through
    // its optional platform package without one.
    run.run(
        "npm",
        &["ci", "--ignore-scripts", "--no-audit", "--no-fund"],
        Some(scratch),
        &[],
    )?;
    let mut args = vec!["entry.js", "--outfile=out.js", "--metafile=meta.json"];
    args.extend(ESBUILD);
    run.run("node_modules/.bin/esbuild", &args, Some(scratch), &[])?;
    let body = std::fs::read_to_string(scratch.join("out.js")).context("read out.js")?;
    let meta = std::fs::read_to_string(scratch.join("meta.json")).context("read meta.json")?;
    Ok((body, licenses(scratch, &meta)?))
}

/// The licence of each package an input of the esbuild metafile `meta` comes from, read from
/// `scratch/node_modules`.
fn licenses(scratch: &Path, meta: &str) -> anyhow::Result<String> {
    let meta: serde_json::Value = serde_json::from_str(meta).context("parse meta.json")?;
    let inputs = meta["inputs"]
        .as_object()
        .context("meta.json has no inputs")?;
    let packages: BTreeSet<String> = inputs.keys().filter_map(|path| package_of(path)).collect();
    let mut out = String::from(
        "The licences of the packages `cargo xtask vendor-tiptap` bundles into tiptap.js.\n",
    );
    for package in packages {
        let root = scratch.join("node_modules").join(&package);
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("package.json"))
                .with_context(|| format!("read {package}'s package.json"))?,
        )?;
        let version = manifest["version"].as_str().unwrap_or("?");
        let license = manifest["license"].as_str().unwrap_or("?");
        let text = license_text(&root).with_context(|| format!("{package} ships no licence"))?;
        out.push_str(&format!(
            "\n---\n\n{package}@{version} ({license})\n\n{}\n",
            text.trim_end()
        ));
    }
    Ok(out)
}

/// The package a metafile input path lies in: `node_modules/@tiptap/core/dist/index.js` is
/// `@tiptap/core`.
fn package_of(path: &str) -> Option<String> {
    let rest = &path[path.rfind("node_modules/")? + "node_modules/".len()..];
    let mut segments = rest.split('/');
    let first = segments.next()?;
    if first.starts_with('@') {
        Some(format!("{first}/{}", segments.next()?))
    } else {
        Some(first.to_string())
    }
}

/// The text of the `LICENSE` file at the root of the package `root`, whatever its case and
/// extension.
fn license_text(root: &Path) -> Option<String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    let name = name.to_ascii_uppercase();
                    name.starts_with("LICENSE") || name.starts_with("LICENCE")
                })
        })
        .collect();
    files.sort();
    std::fs::read_to_string(files.first()?).ok()
}

/// Fails when `tiptap.js` was not built from the files beside it, or the bundle or its licences
/// changed since: a bumped package or a new entry without a rebuild, or a hand edit.
pub fn verify_tiptap() -> anyhow::Result<()> {
    let dir = tiptap_dir();
    let bundle = std::fs::read_to_string(dir.join("tiptap.js")).context("read tiptap.js")?;
    let licenses =
        std::fs::read_to_string(dir.join(LICENSES)).with_context(|| format!("read {LICENSES}"))?;
    let (first, body) = bundle.split_once('\n').unwrap_or((&bundle, ""));
    anyhow::ensure!(
        first == header(&inputs_hash(&dir)?, body, &licenses),
        "tiptap.js was not built from the files beside it, or was edited since; run \
         `cargo xtask vendor-tiptap`"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::package_of;

    #[test]
    fn an_input_lies_in_its_innermost_package() {
        assert_eq!(
            package_of("node_modules/@tiptap/core/dist/index.js").as_deref(),
            Some("@tiptap/core")
        );
        assert_eq!(
            package_of("node_modules/@tiptap/pm/node_modules/orderedmap/dist/index.js").as_deref(),
            Some("orderedmap")
        );
        assert_eq!(package_of("entry.js"), None);
    }
}
