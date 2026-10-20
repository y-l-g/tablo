//! [`MountError`], every mistake a refused panel declares.

use std::fmt;

use super::DeclarationError;

/// Why [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel) refused a panel:
/// every declaration mistake it found.
///
/// The router's builder returns it as a [`topcoat::Error`]; recover it with
/// `error.downcast_ref::<MountError>()`.
#[derive(Debug)]
pub struct MountError {
    panel: String,
    errors: Vec<DeclarationError>,
}

impl MountError {
    pub(crate) fn new(panel: &str, errors: Vec<DeclarationError>) -> Self {
        Self {
            panel: panel.to_string(),
            errors,
        }
    }

    /// The refused panel's prefix.
    pub fn panel(&self) -> &str {
        &self.panel
    }

    /// Every mistake found, in declaration order.
    pub fn errors(&self) -> &[DeclarationError] {
        &self.errors
    }
}

impl fmt::Display for MountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "panel '{}' cannot mount:", self.panel)?;
        for error in &self.errors {
            write!(f, "\n  - {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for MountError {}
