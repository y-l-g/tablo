//! What an [`AuthConfig`] answers about two-factor sign-in, with or without the `two-factor`
//! feature.

use super::AuthConfig;
#[cfg(feature = "two-factor")]
use crate::auth::TwoFactor;

impl AuthConfig {
    /// The two-factor sign-in, or `None` when the panel asks for one factor.
    #[cfg(feature = "two-factor")]
    pub(crate) fn two_factor_config(&self) -> Option<&TwoFactor> {
        self.authenticator.as_ref().and(self.two_factor.as_ref())
    }

    /// Whether the panel asks for a second factor; never without the `two-factor` feature.
    pub(crate) fn asks_two_factor(&self) -> bool {
        #[cfg(feature = "two-factor")]
        {
            self.two_factor_config().is_some()
        }
        #[cfg(not(feature = "two-factor"))]
        {
            false
        }
    }

    /// Whether the declaration installs two-factor sign-in, signing anyone in or not.
    pub(super) fn two_factor_declared(&self) -> bool {
        #[cfg(feature = "two-factor")]
        {
            self.two_factor.is_some()
        }
        #[cfg(not(feature = "two-factor"))]
        {
            false
        }
    }

    /// Whether the declared two-factor sign-in mails codes.
    pub(super) fn two_factor_mails(&self) -> bool {
        #[cfg(feature = "two-factor")]
        {
            self.two_factor
                .as_ref()
                .is_some_and(|two_factor| two_factor.mails())
        }
        #[cfg(not(feature = "two-factor"))]
        {
            false
        }
    }
}
