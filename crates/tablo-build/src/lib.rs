//! Build-script helpers: builds the app's stylesheet on Tablo's default theme, scanning Tablo's
//! sources (published via Cargo `links` metadata) for the classes the app cannot name.
//!
//! In the app's `build.rs` `main`:
//!
//! ```no_run
//! tablo_build::tailwind().expect("the Tailwind build runs");
//! ```
//!
//! The package must depend on `tablo` (or on `tablo-core` and `tablo-ui`) as a
//! normal dependency, and name this crate under `[build-dependencies]`.
//!
//! An app on the default theme that writes no Tailwind classes of its own needs no build:
//! `tablo-ui`'s `DEFAULT_STYLESHEET` is Tablo's sources built on the default theme.

use std::{
    env,
    ffi::OsString,
    fmt, fs, io,
    path::{Path, PathBuf},
};

/// Holds one published source directory each, as Cargo names them for a
/// dependent's build script.
const SOURCE_VARS: &[&str] = &[
    "DEP_TABLO_CORE",
    "DEP_TABLO_UI",
    "DEP_TABLO_AUDIT",
    "DEP_TABLO_CORE_SRC",
    "DEP_TABLO_UI_SRC",
];

/// The end of the variable a crate that extends panels publishes its source directory as: one
/// with `links = "acme-reports"` and `cargo::metadata=tablo_src=…` reaches its dependent's build
/// script as `DEP_ACME_REPORTS_TABLO_SRC`, and its markup's classes are scanned too.
const EXTENSION_SUFFIX: &str = "_TABLO_SRC";

/// The stylesheet [`tailwind`] reads, relative to the package root.
pub const STYLESHEET: &str = "styles.css";

/// Tablo's design tokens, which the app's stylesheet overrides.
const THEME: &str = include_str!("theme.css");

/// Why [`tailwind`] failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// No Tablo crate published its sources to this build script: the package
    /// does not depend on `tablo`, `tablo-core` or `tablo-ui` directly.
    NoSources,
    /// A Cargo variable a build script always has is unset: the function ran
    /// outside a build script.
    NotABuildScript(&'static str),
    /// Reading the stylesheet or writing the generated input failed.
    Io { path: PathBuf, source: io::Error },
    /// The stylesheet imports `tailwindcss` itself; [`tailwind`] imports it ahead of the theme.
    ImportsTailwind(PathBuf),
    /// The Tailwind CLI could not be fetched or failed.
    Tailwind(topcoat::tailwind::BuildError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSources => write!(
                f,
                "no Tablo sources to scan: none of {} is set; depend on `tablo` (or `tablo-core` \
                 and `tablo-ui`) under [dependencies]",
                SOURCE_VARS.join(", ")
            ),
            Self::NotABuildScript(var) => {
                write!(f, "`{var}` is not set; call this from a build script")
            }
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::ImportsTailwind(path) => write!(
                f,
                "{} imports `tailwindcss`; tablo_build imports it ahead of Tablo's default theme, \
                 so the stylesheet keeps only its `@source` lines and the tokens it overrides",
                path.display()
            ),
            Self::Tailwind(error) => write!(f, "tailwind: {error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Tailwind(error) => Some(error),
            Self::NoSources | Self::NotABuildScript(_) | Self::ImportsTailwind(_) => None,
        }
    }
}

impl From<topcoat::tailwind::BuildError> for Error {
    fn from(error: topcoat::tailwind::BuildError) -> Self {
        Self::Tailwind(error)
    }
}

/// Builds the app's stylesheet with Tablo's sources in scope, writing
/// `$OUT_DIR/tailwind.css` and returning that path.
///
/// The build imports Tailwind, then Tablo's default theme, then the app's `styles.css`, so the
/// stylesheet holds only its own `@source` lines and the tokens it redeclares.
///
/// Prints `cargo::rerun-if-changed` for the stylesheet and each source directory.
///
/// # Errors
///
/// [`Error::NotABuildScript`] outside a build script, [`Error::NoSources`]
/// when no Tablo crate published its sources to this build script,
/// [`Error::Io`] when the stylesheet is missing or the input cannot be
/// written, [`Error::ImportsTailwind`] when the stylesheet imports Tailwind
/// itself, and [`Error::Tailwind`] when the CLI fails.
pub fn tailwind() -> Result<PathBuf, Error> {
    let manifest_dir = var("CARGO_MANIFEST_DIR")?;
    let out_dir = PathBuf::from(var("OUT_DIR")?);
    let stylesheet = PathBuf::from(manifest_dir).join(STYLESHEET);
    if !stylesheet.is_file() {
        return Err(Error::Io {
            path: stylesheet,
            source: io::Error::new(io::ErrorKind::NotFound, "the app's stylesheet is missing"),
        });
    }
    let app_css = fs::read_to_string(&stylesheet).map_err(|source| Error::Io {
        path: stylesheet.clone(),
        source,
    })?;
    if imports_tailwind(&app_css) {
        return Err(Error::ImportsTailwind(stylesheet));
    }
    let sources = source_dirs(|name| env::var_os(name), extension_dirs());
    if sources.is_empty() {
        return Err(Error::NoSources);
    }
    println!("cargo::rerun-if-changed={}", stylesheet.display());
    for dir in &sources {
        println!("cargo::rerun-if-changed={}", dir.display());
    }
    let theme = out_dir.join("tablo-theme.css");
    write_if_changed(&theme, THEME)?;
    let input = out_dir.join("tablo-tailwind-input.css");
    write_if_changed(&input, &input_css(&theme, &stylesheet, &sources))?;
    Ok(topcoat::tailwind::BuildConfig::new()
        .input(input)
        .render()?)
}

/// The file [`default_stylesheet`] writes in `OUT_DIR`.
#[doc(hidden)]
pub const DEFAULT_STYLESHEET: &str = "tablo-default.css";

/// Builds Tablo's prebuilt stylesheet, `tablo-ui`'s `DEFAULT_STYLESHEET`: Tailwind over Tablo's
/// sources alone, on the default theme, written to `$OUT_DIR/`[`DEFAULT_STYLESHEET`]. Tablo's
/// repository regenerates its committed copy with it; an app runs [`tailwind`].
///
/// # Errors
///
/// As [`tailwind`], less the stylesheet's.
#[doc(hidden)]
pub fn default_stylesheet() -> Result<PathBuf, Error> {
    let out_dir = PathBuf::from(var("OUT_DIR")?);
    let sources = source_dirs(|name| env::var_os(name), extension_dirs());
    if sources.is_empty() {
        return Err(Error::NoSources);
    }
    let theme = out_dir.join("tablo-theme.css");
    write_if_changed(&theme, THEME)?;
    let input = out_dir.join("tablo-default-input.css");
    write_if_changed(&input, &default_input_css(&theme, &sources))?;
    Ok(topcoat::tailwind::BuildConfig::new()
        .input(input)
        .output(out_dir.join(DEFAULT_STYLESHEET))
        .cwd(out_dir)
        .render()?)
}

fn var(name: &'static str) -> Result<OsString, Error> {
    env::var_os(name).ok_or(Error::NotABuildScript(name))
}

/// The directories the crates that extend panels publish, by [`EXTENSION_SUFFIX`], in the order
/// of their variables' names.
fn extension_dirs() -> Vec<OsString> {
    let mut vars: Vec<(OsString, OsString)> = env::vars_os()
        .filter(|(name, _)| {
            name.to_str()
                .is_some_and(|name| name.starts_with("DEP_") && name.ends_with(EXTENSION_SUFFIX))
        })
        .collect();
    vars.sort();
    vars.into_iter().map(|(_, value)| value).collect()
}

/// Collects the published directories, Tablo's then `extensions`, deduplicated in first-seen
/// order.
fn source_dirs(
    lookup: impl Fn(&str) -> Option<OsString>,
    extensions: impl IntoIterator<Item = OsString>,
) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for value in SOURCE_VARS
        .iter()
        .filter_map(|name| lookup(name))
        .chain(extensions)
    {
        let dir = PathBuf::from(value);
        if !dir.as_os_str().is_empty() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// Whether an `@import` in `css` names `tailwindcss`.
fn imports_tailwind(css: &str) -> bool {
    css.lines().any(|line| {
        line.trim_start()
            .strip_prefix("@import")
            .is_some_and(|rest| {
                let rest = rest.trim_start();
                rest.starts_with("\"tailwindcss") || rest.starts_with("'tailwindcss")
            })
    })
}

/// Tailwind, then the theme, then the app's stylesheet: a later `:root` token wins.
fn input_css(theme: &Path, stylesheet: &Path, sources: &[PathBuf]) -> String {
    let mut css = format!(
        "@import \"tailwindcss\";\n@import {};\n@import {};\n",
        css_string(theme),
        css_string(stylesheet)
    );
    for dir in sources {
        css.push_str(&format!("@source {};\n", css_string(&dir.join("**/*.rs"))));
    }
    css
}

/// Tailwind with its own source detection off, then the theme, then Tablo's sources alone: the
/// output holds no class of the package that builds it.
fn default_input_css(theme: &Path, sources: &[PathBuf]) -> String {
    let mut css = format!(
        "@import \"tailwindcss\" source(none);\n@import {};\n",
        css_string(theme)
    );
    for dir in sources {
        css.push_str(&format!("@source {};\n", css_string(&dir.join("**/*.rs"))));
    }
    css
}

fn css_string(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    format!("\"{}\"", path.replace('"', "\\\""))
}

fn write_if_changed(path: &Path, contents: &str) -> Result<(), Error> {
    if fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    fs::write(path, contents).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests;
