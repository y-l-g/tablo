//! Why a panel cannot serve a directory.

use std::fmt;

/// Why a panel cannot serve a directory [`serve_dir`](crate::Panel::serve_dir) or
/// [`uploads_dir`](crate::Panel::uploads_dir) declares.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServeDirFault {
    /// A pattern that does not end in a catch-all.
    ///
    /// End it in `{*file}`: `/uploads/{*file}`.
    WithoutCatchAll {
        /// The pattern.
        path: String,
    },
    /// The panel serves the same directory pattern twice.
    ///
    /// Drop the second `serve_dir`.
    Twice {
        /// The pattern.
        path: String,
    },
    /// Another panel already serves the directory pattern.
    ///
    /// Serve it from one panel.
    Taken {
        /// The pattern.
        path: String,
        /// The serving panel's prefix.
        panel: String,
    },
}

impl fmt::Display for ServeDirFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WithoutCatchAll { path } => write!(
                f,
                "serve_dir path '{path}' must end in a catch-all like '/uploads/{{*file}}'"
            ),
            Self::Twice { path } => write!(f, "serve_dir path '{path}' is declared twice"),
            Self::Taken { path, panel } => write!(
                f,
                "serve_dir path '{path}' is already served by the panel mounted at '{panel}'"
            ),
        }
    }
}
