//! What an [`AuthConfig`] answers about its throttles: which it declares, which count in the
//! database, and which the mount refuses.

use super::AuthConfig;
use crate::auth::LoginThrottle;

impl AuthConfig {
    /// Whether any of the panel's throttles counts in the database, which then needs
    /// [`AuthThrottle`](super::AuthThrottle).
    pub(crate) fn throttles_in_database(&self) -> bool {
        self.throttles()
            .any(|(_, throttle)| throttle.counts_in_database())
    }

    /// Makes every throttle declared so far count in the database.
    pub(super) fn count_throttles_in_database(&mut self) {
        for throttle in [
            &mut self.throttle,
            &mut self.account_throttle,
            &mut self.sign_up_throttle,
            &mut self.profile_throttle,
        ] {
            throttle.count_in_database();
        }
        if let Some(reset) = &mut self.reset {
            reset.throttle.count_in_database();
        }
        if let Some(verification) = &mut self.verification {
            verification.throttle.count_in_database();
        }
        #[cfg(feature = "two-factor")]
        if let Some(two_factor) = &mut self.two_factor {
            two_factor.throttle.count_in_database();
            two_factor.mail_throttle.count_in_database();
        }
    }

    /// The panel's throttles whose limit allows no sign-in, each refused by the mount.
    pub(crate) fn throttle_declaration_errors(&self) -> Vec<crate::DeclarationErrorKind> {
        self.throttles()
            .filter(|(_, throttle)| throttle.allows_none())
            .map(|(builder, _)| crate::DeclarationErrorKind::ThrottleAllowsNone { builder })
            .collect()
    }

    /// Every throttle the panel declares, with the builder call that sets it.
    fn throttles(&self) -> impl Iterator<Item = (&'static str, &LoginThrottle)> {
        let reset = self
            .reset()
            .map(|reset| ("PasswordReset::throttle", &reset.throttle));
        let verification = self
            .verification()
            .map(|verification| ("EmailVerification::throttle", &verification.throttle));
        #[cfg(feature = "two-factor")]
        let two_factor = self.two_factor_config().into_iter().flat_map(|two_factor| {
            [
                ("TwoFactor::throttle", &two_factor.throttle),
                ("TwoFactor::mail_throttle", &two_factor.mail_throttle),
            ]
        });
        #[cfg(not(feature = "two-factor"))]
        let two_factor = std::iter::empty();
        [
            ("Auth::throttle", &self.throttle),
            ("Auth::account_throttle", &self.account_throttle),
            ("Auth::sign_up_throttle", &self.sign_up_throttle),
            ("Auth::profile_throttle", &self.profile_throttle),
        ]
        .into_iter()
        .chain(reset)
        .chain(verification)
        .chain(two_factor)
    }
}
