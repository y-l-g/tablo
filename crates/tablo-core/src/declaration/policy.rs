//! Why a panel's content security policy cannot be served.

use std::fmt;

/// Why a panel's [`content_security_policy`](crate::Panel::content_security_policy) or a
/// [`content_security_policy_source`](crate::Panel::content_security_policy_source) cannot be
/// served.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PolicyFault {
    /// A `Panel::content_security_policy` that is no valid header value: drop its line breaks.
    InvalidHeader {
        /// The policy.
        policy: String,
    },
    /// A `Panel::content_security_policy_source` the panel's policy cannot carry: a directive
    /// that is no lowercase name, a source that is empty or holds whitespace, `;` or `,`, a
    /// directive the policy leaves out that is no fetch directive, or any source on a panel
    /// without a policy.
    ///
    /// Fix the source, or set the whole policy with `content_security_policy`.
    InvalidSource {
        /// The directive.
        directive: String,
        /// The source.
        source: String,
    },
}

impl fmt::Display for PolicyFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHeader { policy } => write!(
                f,
                "content_security_policy {policy:?} is not a valid header value"
            ),
            Self::InvalidSource { directive, source } => write!(
                f,
                "content_security_policy_source({directive:?}, {source:?}) cannot be added to \
                 the panel's policy: check the directive and the source, or set the whole policy \
                 with `content_security_policy`"
            ),
        }
    }
}
