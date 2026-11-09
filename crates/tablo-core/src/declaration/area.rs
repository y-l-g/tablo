//! The declaration mistakes of a panel's [`Area`](crate::Area)s.

use std::fmt;

/// Why an area cannot hold one of the pages auth serves:
/// [`DeclarationErrorKind::Area`](super::DeclarationErrorKind::Area). Each names the page:
/// `profile`, `two-factor` or `tenant settings`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AreaFault {
    /// An area holds a page the panel's auth does not install.
    ///
    /// Install it on the panel's `Auth`, or take it out of the area.
    AuthPageNotOffered(&'static str),
    /// Two groups hold the same page auth serves, which mounts at one URL.
    ///
    /// Keep one.
    AuthPagePlacedTwice(&'static str),
    /// The def of a page auth serves declares what its page does not render: widgets, filters,
    /// an access check or header actions.
    ///
    /// Drop it from the def.
    AuthPageDeclares {
        /// The page.
        page: &'static str,
        /// What the def declares.
        feature: &'static str,
    },
    /// An area holds the audit log's list, but the panel keeps no audit log.
    ///
    /// Call `Panel::audit_log`, or take it out of the area.
    AuditLogNotKept,
    /// Two groups hold the audit log's list, which mounts at one URL.
    ///
    /// Keep one.
    AuditLogPlacedTwice,
}

impl fmt::Display for AreaFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthPageNotOffered(page) => write!(
                f,
                "an area holds the {page} page, which the panel's auth does not install: \
                 install it on the `Auth`, or take it out of the area"
            ),
            Self::AuthPagePlacedTwice(page) => write!(
                f,
                "two area groups hold the {page} page, which mounts at one URL: keep one"
            ),
            Self::AuthPageDeclares { page, feature } => write!(
                f,
                "the def of the {page} page declares {feature}, which the page does not render: \
                 drop them from the def"
            ),
            Self::AuditLogNotKept => f.write_str(
                "an area holds the audit log, which the panel does not keep: call \
                 `Panel::audit_log`, or take it out of the area",
            ),
            Self::AuditLogPlacedTwice => {
                f.write_str("two area groups hold the audit log, which mounts at one URL: keep one")
            }
        }
    }
}
