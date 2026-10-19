//! [`Auth`], the panel's authentication configuration.

#[cfg(feature = "password")]
use super::PasswordAuth;
#[cfg(feature = "mail")]
use super::PasswordReset;
#[cfg(doc)]
use super::{AdminUser, PasswordResetter, Profile, RESET_LIFETIME, SignUp};
use super::{
    Authenticator, DynAuthenticator, LoginThrottle, ProfileEditor, Registrar, password_reset,
    profile::DynProfileEditor, registration::DynRegistrar,
};

/// The panel's authentication configuration (ADR-0013).
///
/// The default is [`Auth::password`]; [`Auth::custom`] installs an app-owned
/// [`Authenticator`]; [`Auth::disabled`] is the explicit fail-open opt-out.
/// Sign-in attempts are throttled per login by default; see [`Auth::throttle`].
/// [`Auth::registration`] adds a sign-up page, [`Auth::profile`] a profile page, and
/// [`Auth::password_reset`] a forgot-password page that mails reset links.
pub struct Auth {
    authenticator: Option<Box<dyn DynAuthenticator>>,
    throttle: LoginThrottle,
    pub(super) registrar: Option<Box<dyn DynRegistrar>>,
    sign_up_throttle: LoginThrottle,
    pub(super) profile_editor: Option<Box<dyn DynProfileEditor>>,
    profile_throttle: LoginThrottle,
    pub(super) reset: Option<password_reset::PasswordReset>,
}

impl Auth {
    /// The shipped Argon2id + [`AdminUser`] authenticator.
    #[cfg(feature = "password")]
    #[must_use]
    pub fn password() -> Self {
        Self::custom(PasswordAuth)
    }

    /// An app-owned authenticator over its own user table.
    #[must_use]
    pub fn custom(authenticator: impl Authenticator) -> Self {
        Self {
            authenticator: Some(Box::new(authenticator)),
            throttle: LoginThrottle::default(),
            registrar: None,
            sign_up_throttle: LoginThrottle::default(),
            profile_editor: None,
            profile_throttle: LoginThrottle::default(),
            reset: None,
        }
    }

    /// Explicit fail-open opt-out for public demos (ADR-0013): no gate, no
    /// login routes, sessions unused.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            authenticator: None,
            throttle: LoginThrottle::off(),
            registrar: None,
            sign_up_throttle: LoginThrottle::off(),
            profile_editor: None,
            profile_throttle: LoginThrottle::off(),
            reset: None,
        }
    }

    /// How many sign-in attempts one login may make from one client address
    /// in a window; defaults to five a minute. [`LoginThrottle::off`] counts nothing, for a proxy
    /// that limits sign-ins itself.
    #[must_use]
    pub fn throttle(mut self, throttle: LoginThrottle) -> Self {
        self.throttle = throttle;
        self
    }

    /// Lets visitors create their own account at `{prefix}/register`, through `registrar`; the
    /// login page links to it.
    ///
    /// ```rust
    /// use tablo_core::{Auth, PasswordAuth};
    ///
    /// let auth = Auth::password().registration(PasswordAuth);
    /// # let _ = auth;
    /// ```
    ///
    /// [`PasswordAuth`] registers an active [`AdminUser`] from a [`SignUp`], who signs in to the
    /// panel at once: only open it where any visitor may hold an account. A custom
    /// [`Registrar`] decides what a new account may do. Mounting refuses a registrar whose
    /// [`User`](Registrar::User) is not the type the authenticator loads, and a registration
    /// on [`Auth::disabled`].
    #[must_use]
    pub fn registration(mut self, registrar: impl Registrar) -> Self {
        self.registrar = Some(Box::new(registrar));
        self
    }

    /// How many sign-ups one client address may submit in a window; defaults to five a minute.
    /// Every sign-up POST that passes the CSRF check and parses counts, whether or not it creates
    /// an account.
    #[must_use]
    pub fn sign_up_throttle(mut self, throttle: LoginThrottle) -> Self {
        self.sign_up_throttle = throttle;
        self
    }

    /// Lets the signed-in user edit their own account at `{prefix}/profile`, through `editor`;
    /// the user menu links to it.
    ///
    /// ```rust
    /// use tablo_core::{Auth, PasswordAuth};
    ///
    /// let auth = Auth::password().profile(PasswordAuth);
    /// # let _ = auth;
    /// ```
    ///
    /// [`PasswordAuth`] edits an [`AdminUser`]'s name, email and password through a [`Profile`],
    /// asking for the current password before the email or the password changes. Mounting
    /// refuses an editor whose [`User`](ProfileEditor::User) is not the type the authenticator
    /// loads, and a profile on [`Auth::disabled`].
    #[must_use]
    pub fn profile(mut self, editor: impl ProfileEditor) -> Self {
        self.profile_editor = Some(Box::new(editor));
        self
    }

    /// How many profile saves one user may submit in a window, from any address; defaults to five
    /// a minute. Every profile POST that passes the CSRF check and parses counts, whether or not
    /// it saves.
    #[must_use]
    pub fn profile_throttle(mut self, throttle: LoginThrottle) -> Self {
        self.profile_throttle = throttle;
        self
    }

    /// Lets a visitor who forgot their password ask for a reset link at
    /// `{prefix}/forgot-password`, mailed through the router's
    /// [`MailConfig`](topcoat::mail::MailConfig), and choose a new password at
    /// `{prefix}/reset-password`; the login page links to it.
    ///
    /// The request page answers the same whether an account uses the address or not. A link
    /// works once, for [`RESET_LIFETIME`], on this panel. Using it ends every session and reset
    /// link the account holds on each panel signing in its user type. Mounting refuses a reset
    /// whose [`User`](PasswordResetter::User) is not the type the authenticator loads, a reset
    /// on [`Auth::disabled`], a base URL that is not absolute, and a router without a
    /// `MailConfig`, which the app registers with `.mail(..)` before mounting the panel.
    #[cfg(feature = "mail")]
    #[must_use]
    pub fn password_reset(mut self, reset: PasswordReset) -> Self {
        self.reset = Some(reset);
        self
    }

    /// Whether the explicit opt-out is set.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.authenticator.is_none()
    }

    /// The authenticator, or `None` when auth is disabled.
    pub(crate) fn authenticator(&self) -> Option<&dyn DynAuthenticator> {
        self.authenticator.as_deref()
    }

    /// The sign-in throttle.
    pub(crate) fn login_throttle(&self) -> &LoginThrottle {
        &self.throttle
    }

    /// The registrar, or `None` without a sign-up page.
    pub(crate) fn registrar(&self) -> Option<&dyn DynRegistrar> {
        self.authenticator.as_ref().and(self.registrar.as_deref())
    }

    /// The sign-up throttle, counted per client address.
    pub(crate) fn throttle_sign_ups(&self) -> &LoginThrottle {
        &self.sign_up_throttle
    }

    /// The profile editor, or `None` without a profile page.
    pub(crate) fn profile_editor(&self) -> Option<&dyn DynProfileEditor> {
        self.authenticator
            .as_ref()
            .and(self.profile_editor.as_deref())
    }

    /// The profile throttle, counted per user.
    pub(crate) fn throttle_profile(&self) -> &LoginThrottle {
        &self.profile_throttle
    }

    /// The password reset, or `None` without a forgot-password page.
    pub(crate) fn reset(&self) -> Option<&password_reset::PasswordReset> {
        self.authenticator.as_ref().and(self.reset.as_ref())
    }
}

/// The auth a panel mounts with: `auth` when it names one, else the default, which only the
/// `password` feature supplies.
pub(crate) fn or_default(auth: Option<Auth>) -> Result<Auth, crate::DeclarationErrorKind> {
    match auth {
        Some(auth) => Ok(auth),
        #[cfg(feature = "password")]
        None => Ok(Auth::password()),
        #[cfg(not(feature = "password"))]
        None => Err(crate::DeclarationErrorKind::NoAuth),
    }
}

#[cfg(feature = "password")]
impl Default for Auth {
    fn default() -> Self {
        Self::password()
    }
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_disabled() {
            f.write_str("Auth::disabled")
        } else {
            f.debug_struct("Auth")
                .field("throttle", &self.throttle)
                .field("registration", &self.registrar.is_some())
                .field("profile", &self.profile_editor.is_some())
                .field("password_reset", &self.reset)
                .finish_non_exhaustive()
        }
    }
}
