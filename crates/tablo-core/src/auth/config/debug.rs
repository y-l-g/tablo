//! How an [`Auth`] prints: what it turns on, never the seams' own state.

use super::{Auth, AuthConfig};

impl<U> std::fmt::Debug for Auth<U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.config.fmt(f)
    }
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_disabled() {
            f.write_str("Auth::disabled")
        } else {
            f.debug_struct("Auth")
                .field("throttle", &self.throttle)
                .field("account_throttle", &self.account_throttle)
                .field("registration", &self.registrar.is_some())
                .field("profile", &self.profile_editor.is_some())
                .field("password_reset", &self.reset)
                .field("email_verification", &self.verification)
                .field("tenant_registration", &self.tenant_registration)
                .field("tenant_profile", &self.tenant_editor.is_some())
                .field("two_factor", &self.two_factor)
                .field("impersonation", &self.impersonation)
                .field("oidc", &self.oidc)
                .field("mail", &self.mail)
                .field("sessions", &self.sessions)
                .finish_non_exhaustive()
        }
    }
}
