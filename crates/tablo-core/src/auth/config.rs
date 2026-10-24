//! [`Auth`]: the panel's authentication configuration, from the authenticator to the pages and
//! mail it adds.

use std::marker::PhantomData;

use super::{
    AdminUser, AuthMail, Authenticator, DynAuthenticator, EmailVerification, LoginThrottle,
    PanelUser, PasswordAuth, PasswordReset, ProfileEditor, Registrar, TenantEditor,
    TenantRegistration, TwoFactor, profile::DynProfileEditor, registration::DynRegistrar,
    tenant_profile::DynTenantEditor,
};

/// The panel's authentication configuration (ADR-0013).
///
/// The default is [`Auth::password`]; [`Auth::custom`] installs an app-owned
/// [`Authenticator`]; [`Auth::disabled`] is the explicit fail-open opt-out.
/// Sign-in attempts are throttled per login and address by default; see [`Auth::throttle`].
/// [`Auth::account_throttle`] adds an opt-in count per login across addresses.
/// [`Auth::registration`] adds a sign-up page, [`Auth::profile`] a profile page, and
/// [`Auth::password_reset`] a forgot-password page that mails reset links through
/// [`Auth::mail`], [`Auth::tenant_registration`] a page creating a tenant, and
/// [`Auth::tenant_profile`] a page editing one.
///
/// `U` is the user type the authenticator loads. Every seam that reads or writes a user, from the
/// registrar to the tenant editor, names the same type, so a seam over another user type does not
/// compile.
pub struct Auth<U = AdminUser> {
    config: AuthConfig,
    user: PhantomData<fn() -> U>,
}

impl Auth {
    /// The shipped Argon2id + [`AdminUser`] authenticator.
    #[must_use]
    pub fn password() -> Self {
        Self::custom(PasswordAuth)
    }

    /// Explicit fail-open opt-out for public demos (ADR-0013): no gate, no
    /// login routes, sessions unused.
    #[must_use]
    pub fn disabled() -> Self {
        Self::new(AuthConfig {
            authenticator: None,
            throttle: LoginThrottle::off(),
            account_throttle: LoginThrottle::off(),
            registrar: None,
            sign_up_throttle: LoginThrottle::off(),
            profile_editor: None,
            profile_throttle: LoginThrottle::off(),
            reset: None,
            mail: None,
            verification: None,
            tenant_registration: None,
            tenant_editor: None,
            two_factor: None,
        })
    }
}

impl<U: PanelUser> Auth<U> {
    /// An app-owned authenticator over its own user table.
    #[must_use]
    pub fn custom<A: Authenticator<User = U>>(authenticator: A) -> Self {
        Self::new(AuthConfig {
            authenticator: Some(Box::new(authenticator)),
            throttle: LoginThrottle::default(),
            account_throttle: LoginThrottle::off(),
            registrar: None,
            sign_up_throttle: LoginThrottle::default(),
            profile_editor: None,
            profile_throttle: LoginThrottle::default(),
            reset: None,
            mail: None,
            verification: None,
            tenant_registration: None,
            tenant_editor: None,
            two_factor: None,
        })
    }

    fn new(config: AuthConfig) -> Self {
        Self {
            config,
            user: PhantomData,
        }
    }

    /// How many sign-in attempts one login may make from one client address
    /// in a window; defaults to five a minute. [`LoginThrottle::off`] counts nothing, for a proxy
    /// that limits sign-ins itself.
    #[must_use]
    pub fn throttle(mut self, throttle: LoginThrottle) -> Self {
        self.config.throttle = throttle;
        self
    }

    /// How many sign-in attempts one login may make from all addresses together in a window;
    /// off by default. It stops a guesser who rotates addresses, at the cost of letting anyone
    /// who knows a login refuse its owner until the window ends, again each window; a successful
    /// sign-in clears it. Turn it on where a guessed password costs more than a locked-out
    /// owner, with a limit high enough that only a distributed guesser reaches it, such as
    /// `LoginThrottle::new(50, Duration::from_hours(1))`.
    #[must_use]
    pub fn account_throttle(mut self, throttle: LoginThrottle) -> Self {
        self.config.account_throttle = throttle;
        self
    }

    /// Lets visitors create their own account at `{prefix}/register`, through `registrar`; the
    /// login page links to it.
    ///
    /// ```rust
    /// use tablo_core::{Auth, PasswordSignUp};
    ///
    /// let auth = Auth::password().registration(PasswordSignUp::pending());
    /// # let _ = auth;
    /// ```
    ///
    /// [`PasswordSignUp`](crate::PasswordSignUp) registers an [`AdminUser`](super::AdminUser) from
    /// a [`SignUp`](super::SignUp), inactive until an administrator approves it or, when built
    /// with `open`, active and signed in at once. A custom [`Registrar`] decides what a new
    /// account may do; one whose user may not access the panel lands on the login page.
    ///
    /// - The form is any `FormInput`, or the shipped `SignUp`. `#[form(password)]` renders a masked
    ///   control, with a button that shows what was typed, that a refused form renders empty, and
    ///   reads the password as typed, outer spaces included, as the login compares it.
    ///   `#[form(email)]` refuses a malformed address before `validate` runs. A missing field and a
    ///   malformed value answer 422 with the error under the field, as `validate`'s refusals do;
    ///   nothing is written.
    /// - `new_password_errors` refuses a password under eight characters and a confirmation that
    ///   differs, under the `password` and `password_confirmation` keys.
    /// - After the commit, an account that may open the panel is signed in, its session rotated as
    ///   a login rotates it, and lands on `next` or the panel's home page. One that may not, such
    ///   as an account awaiting approval, lands on the login page with a notification instead. A
    ///   signed-in user who opens the sign-up page lands on the panel's home page.
    /// - The registrar's `User` is `U`. Mounting refuses a registration on `Auth::disabled()`, a
    ///   form field named `csrf_token` or `next`, a file field and a relationship choice, whose
    ///   options would list records to a visitor nobody signed in.
    /// - Throttling. Each client address may submit five sign-ups a minute, counted whether or not
    ///   they create an account; `Auth::sign_up_throttle` changes the limit. Past it the page
    ///   answers 429.
    ///
    /// The sign-up page tells a visitor whether an email is taken, which the login page never does:
    /// an app that must keep its accounts private verifies the address by email before saying so,
    /// which Tablo does not ship.
    #[must_use]
    pub fn registration(mut self, registrar: impl Registrar<User = U>) -> Self {
        self.config.registrar = Some(Box::new(registrar));
        self
    }

    /// How many sign-ups one client address may submit in a window; defaults to five a minute.
    /// Every sign-up POST that passes the CSRF check and parses counts, whether or not it creates
    /// an account.
    #[must_use]
    pub fn sign_up_throttle(mut self, throttle: LoginThrottle) -> Self {
        self.config.sign_up_throttle = throttle;
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
    /// [`PasswordAuth`] edits an [`AdminUser`](super::AdminUser)'s name, email and password through
    /// a [`Profile`](super::Profile), asking for the current password before the email or the
    /// password changes.
    ///
    /// - The form is any `FormInput`, rendered in the panel's shell. The page shows what the
    ///   input's `write` writes, so `Profile` never shows a password. A disabled control keeps what
    ///   `fill` wrote, whatever the POST carries. A refused save renders the form again with each
    ///   error under its field; nothing is written.
    /// - `save` answers `ProfileUpdate::Details`, which keeps every session, or
    ///   `ProfileUpdate::Credentials` for a change to what the user signs in with. The user's
    ///   sessions then end on every panel whose authenticator loads the same user type, in the
    ///   transaction that saves, and the browser that saved gets a new session under the tenant it
    ///   acted for. `PasswordAuth` answers `Credentials` for a new email or password.
    /// - An email another account holds is reported only once the current password verifies, so a
    ///   stolen session cannot probe which addresses have accounts.
    /// - After the commit the page reloads with a notification.
    /// - The editor's `User` is `U`. Mounting refuses a profile on `Auth::disabled()`, a form
    ///   field named `csrf_token`, a file field and a relationship choice, whose options no route
    ///   serves the page.
    /// - Throttling. Each user may submit five saves a minute, from any address, counted whether or
    ///   not they save; `Auth::profile_throttle` changes the limit. Past it the page answers 429,
    ///   which keeps a stolen session from guessing the current password. A user past the limit can
    ///   still sign in.
    #[must_use]
    pub fn profile(mut self, editor: impl ProfileEditor<User = U>) -> Self {
        self.config.profile_editor = Some(Box::new(editor));
        self
    }

    /// How many profile saves one user may submit in a window, from any address; defaults to five
    /// a minute. Every profile POST that passes the CSRF check and parses counts, whether or not
    /// it saves.
    #[must_use]
    pub fn profile_throttle(mut self, throttle: LoginThrottle) -> Self {
        self.config.profile_throttle = throttle;
        self
    }

    /// Lets a visitor who forgot their password ask for a reset link at
    /// `{prefix}/forgot-password`, mailed through [`Auth::mail`], and choose a new password at
    /// `{prefix}/reset-password`; the login page links to it.
    ///
    /// - The request page answers the same whether an account uses the address or not: it redirects
    ///   to the login page with a notification, and writes and mails the link after answering, so
    ///   the database and the mail server add no time when an account exists. An account that may
    ///   not enter the panel gets no link, and a failed send is logged, not shown.
    /// - The link goes to the stored address, never the typed one. A lookup that matches loosely,
    ///   such as a collation that folds accents, would otherwise mail one account's link to another
    ///   address.
    /// - A link works once, for one hour, on the panel that mailed it, and only while the account
    ///   may still enter the panel. A new request replaces the account's earlier link, and a
    ///   password change on the profile page ends it, as it ends every link the account holds. The
    ///   token is stored only as a hash, and the reset page sends no referrer and may not be
    ///   cached.
    /// - A reset checks the new password with `new_password_errors`, then sets it, ends the
    ///   account's sessions and reset links on every panel whose authenticator loads the same user
    ///   type, and redirects to the login page. All of this commits in one transaction.
    /// - Throttling. Each address may request three links in fifteen minutes, from any client,
    ///   counted whether or not an account uses it; `PasswordReset::throttle` changes the limit.
    ///   Past it the page answers 429. A limit per client belongs at the proxy.
    /// - The resetter's `User` is `U`. Mounting refuses a reset on `Auth::disabled()`, a reset
    ///   without `Auth::mail`, a base URL that is not an absolute `http` or `https` URL or that
    ///   carries a query or fragment, and a router without a `MailConfig`.
    #[must_use]
    pub fn password_reset(mut self, reset: PasswordReset<U>) -> Self {
        self.config.reset = Some(reset.erase());
        self
    }

    /// Makes each signed-in user prove they read the address [`PanelUser::email`] answers before
    /// they reach anything but `{prefix}/verify-email`, which mails them a link through
    /// [`Auth::mail`]. A sign-up mails the link at once, after the response.
    ///
    /// - Until the user verifies, a page redirects to `{prefix}/verify-email` and any other request
    ///   answers 403. The page offers to mail a link, and to sign out. A user whose
    ///   `PanelUser::email` is `None` has no address to prove and is not held.
    /// - Accounts that predate verification are unverified: turning it on sends each of them,
    ///   seeded admins included, to `{prefix}/verify-email` at their next request. Backfill
    ///   `email_verified_at` (from `created_at`, say) for the addresses you already trust.
    /// - A link works once, for one day, on the panel that mailed it, and only while the account
    ///   may enter the panel and still stores the address it was mailed to. It verifies on `GET` in
    ///   any browser, signed in or not, then lands on the panel or on the login page. A mail
    ///   scanner that opens the link verifies the address too: it fetched the link from the mailbox
    ///   the link proves.
    /// - A new email waits for its own link. A profile editor calls `auth::request_email_change`
    ///   from `save` in place of writing the address: on a panel with email verification it records
    ///   the change and answers `EmailChange::Pending`, and once the save commits the panel mails
    ///   the new address a link that stores it and the old address a link that keeps it. Elsewhere
    ///   it answers `EmailChange::Immediate` and the editor stores the address itself.
    ///   `PasswordAuth` does this. The link proves only that whoever asked reads the new mailbox,
    ///   so ask for the current password before calling it, as `PasswordAuth` does: a stolen
    ///   session could name its thief's mailbox.
    /// - Either link opens a page whose button does the work, so a mail scanner that fetches it
    ///   changes nothing.
    /// - The new address's link stores it, verified, once, for one day. It is a credential change:
    ///   the account's sessions and sign-in links end on every panel signing in its user type, and
    ///   the browser that confirmed keeps a new session when it was signed in as the account.
    /// - The old address's link works for a week, before or after the change goes through: it keeps
    ///   the old address, restoring it if the change went through, and ends every session, sign-in
    ///   link and pending change the account holds, the cancelling browser's included: whoever
    ///   asked for the change may hold one.
    /// - Throttling. Each account may ask for three links in fifteen minutes;
    ///   `EmailVerification::throttle` changes the limit. Past it the page answers 429.
    /// - The verifier's `User` is `U`. Mounting refuses a verification on `Auth::disabled()`, and
    ///   one without `Auth::mail`.
    ///
    /// [`PanelUser::email`]: super::PanelUser::email
    #[must_use]
    pub fn email_verification(mut self, verification: EmailVerification<U>) -> Self {
        self.config.verification = Some(verification.erase());
        self
    }

    /// Lets the signed-in user create a tenant to act for at `{prefix}/register-tenant`, through
    /// `registration`'s [`TenantRegistrar`](super::TenantRegistrar); the tenant menu links to
    /// it from one membership on. [`TenantRegistration::required`] sends a user who acts for no
    /// tenant there first.
    ///
    /// The registrar's `Input` is the page's form; `validate` and `register` run in one
    /// transaction, and `register` writes the tenant and the user's membership of it, answering
    /// that [`Membership`](crate::Membership). The session then acts for the new tenant, and the
    /// next request's [`find_by_id`](crate::Authenticator::find_by_id) must list it.
    ///
    /// The registrar's [`User`](super::TenantRegistrar::User) is `U`. Mounting refuses a
    /// registration on [`Auth::disabled`], a form field named `csrf_token`, a file field and a
    /// relationship choice.
    #[must_use]
    pub fn tenant_registration(mut self, registration: TenantRegistration<U>) -> Self {
        self.config.tenant_registration = Some(registration.erase());
        self
    }

    /// Lets the signed-in user edit the tenant they act for at `{prefix}/tenant/profile`, through
    /// `editor`; the tenant menu links to it for a user [`TenantEditor::may_edit`] admits, and the
    /// page answers 403 to anyone else. `fill` loads the form from the tenant, and `validate` and
    /// `save` run in one transaction. The form names the tenant it was filled from, so one posted
    /// after another tab switched tenants saves nothing.
    ///
    /// The editor's [`User`](TenantEditor::User) is `U`. Mounting refuses a settings page on
    /// [`Auth::disabled`], a form field named `csrf_token` or `editing_tenant`, a file field and a
    /// relationship choice.
    #[must_use]
    pub fn tenant_profile(mut self, editor: impl TenantEditor<User = U>) -> Self {
        self.config.tenant_editor = Some(Box::new(editor));
        self
    }

    /// Lets each signed-in user add an authenticator app at `{prefix}/two-factor`; the user menu
    /// links to it. An account with one signs in with a code from it, or a recovery code, at
    /// `{prefix}/two-factor-challenge` after its password.
    ///
    /// - Setting up happens at `{prefix}/two-factor`, which the user menu links to. It asks for the
    ///   current password first, through `Authenticator::confirm_password`, so a stolen session
    ///   cannot add a factor its thief holds. That method verifies with the user's
    ///   `PanelUser::email` as the login; override it when users sign in with something else. The
    ///   page then shows a QR code and the key, and turns two-factor sign-in on once a code from
    ///   the app matches. It then shows eight recovery codes, once: the panel stores only their
    ///   hashes, keyed like the secrets.
    /// - Each panel enrols apart. A factor signs in to the panel that set it up only, even when
    ///   other panels load the same user type: enrol on each.
    /// - Signing in, a correct password leads to `{prefix}/two-factor-challenge` instead of a
    ///   session. The challenge lasts five minutes in its own cookie and takes a code from the app
    ///   or a recovery code. Each code works once: an app code does not sign in twice, and a
    ///   recovery code is used up.
    /// - Wrong codes. Every fifth wrong code in a row ends the challenge, so the next try takes the
    ///   password again. Past the fifth, each code of the app or the mail waits for the one before
    ///   it: 30 seconds, doubling up to an hour, across challenges, until a right code; a recovery
    ///   code never waits. Each fifth mails the account, through `Auth::mail`, that its password
    ///   signed in and codes failed. Someone holding the password can so keep the app's codes
    ///   waiting and make the owner spend recovery codes; the mail tells the owner, and a new
    ///   password ends it.
    /// - Turning it on or off is a credential change: the account's other sessions and links end.
    ///   Turning it off, and showing new recovery codes, take a code.
    /// - Codes by email. `TwoFactor::new().email_codes()` lets a user with an address choose codes
    ///   mailed to it in place of an app, through `Auth::mail`. Each sign-in mails a six-digit code
    ///   that works once, for ten minutes; the challenge page mails a new one on request, in place
    ///   of the last. Each account may have three mailed in fifteen minutes;
    ///   `TwoFactor::mail_throttle` changes the limit. An account holds one factor at a time: it
    ///   turns one off before setting up the other. A panel that drops `email_codes()` mails no
    ///   more codes: an account set up for them signs in with a recovery code. A mail server that
    ///   fails leaves the challenge up, with its button to send another code.
    /// - Throttling. Each account may try five codes or passwords a minute, at sign-in and on the
    ///   settings page together; `TwoFactor::throttle` changes the limit. Past it the page answers
    ///   429. The count lives in each process: behind several, the limit holds per process. Someone
    ///   who knows the password can use up the owner's attempts, and each new sign-in replaces the
    ///   last challenge.
    /// - The cookie `Key` seals the secrets. Rotating it makes every app code fail until its owner
    ///   signs in with a recovery code and sets the app up again; a leaked key opens every secret.
    /// - Requiring it. `TwoFactor::new().required()` sends a signed-in user without a second factor
    ///   to `{prefix}/two-factor` before anything else, as an unverified address does: a page
    ///   redirects there and any other request answers 403.
    /// - Mounting refuses two-factor sign-in on `Auth::disabled()`, a router without a cookie
    ///   `Key`, and codes by email without `Auth::mail`.
    #[must_use]
    pub fn two_factor(mut self, two_factor: TwoFactor) -> Self {
        self.config.two_factor = Some(two_factor);
        self
    }

    /// Sends the panel's auth mail, such as reset links, from `mail`'s sender, with links that
    /// start with its base URL. Mounting refuses a base URL that is not absolute, and a router
    /// without a [`MailConfig`](topcoat::mail::MailConfig).
    #[must_use]
    pub fn mail(mut self, mail: AuthMail) -> Self {
        self.config.mail = Some(mail);
        self
    }

    /// Whether the explicit opt-out is set.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.config.is_disabled()
    }

    /// The declaration, its user type checked.
    pub(crate) fn into_config(self) -> AuthConfig {
        self.config
    }
}

impl Default for Auth {
    fn default() -> Self {
        Self::password()
    }
}

impl<U> std::fmt::Debug for Auth<U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.config.fmt(f)
    }
}

/// What an [`Auth`] declares, with the user type erased once the builder has checked it.
pub(crate) struct AuthConfig {
    authenticator: Option<Box<dyn DynAuthenticator>>,
    throttle: LoginThrottle,
    account_throttle: LoginThrottle,
    pub(super) registrar: Option<Box<dyn DynRegistrar>>,
    sign_up_throttle: LoginThrottle,
    pub(super) profile_editor: Option<Box<dyn DynProfileEditor>>,
    profile_throttle: LoginThrottle,
    pub(super) reset: Option<PasswordReset<()>>,
    pub(super) mail: Option<AuthMail>,
    pub(super) verification: Option<EmailVerification<()>>,
    pub(super) tenant_registration: Option<TenantRegistration<()>>,
    pub(super) tenant_editor: Option<Box<dyn DynTenantEditor>>,
    pub(super) two_factor: Option<TwoFactor>,
}

impl AuthConfig {
    /// Whether the explicit opt-out is set.
    pub(crate) fn is_disabled(&self) -> bool {
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

    /// The sign-in throttle counted per login across addresses.
    pub(crate) fn account_throttle_of(&self) -> &LoginThrottle {
        &self.account_throttle
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
    pub(crate) fn reset(&self) -> Option<&PasswordReset<()>> {
        self.authenticator.as_ref().and(self.reset.as_ref())
    }

    /// The email verification, or `None` when the panel asks for none.
    pub(crate) fn verification(&self) -> Option<&EmailVerification<()>> {
        self.authenticator.as_ref().and(self.verification.as_ref())
    }

    /// The tenant registration, or `None` without a tenant registration page.
    pub(crate) fn tenant_registration_page(&self) -> Option<&TenantRegistration<()>> {
        self.authenticator
            .as_ref()
            .and(self.tenant_registration.as_ref())
    }

    /// The tenant editor, or `None` without a tenant settings page.
    pub(crate) fn tenant_editor(&self) -> Option<&dyn DynTenantEditor> {
        self.authenticator
            .as_ref()
            .and(self.tenant_editor.as_deref())
    }

    /// The two-factor sign-in, or `None` when the panel asks for one factor.
    pub(crate) fn two_factor_config(&self) -> Option<&TwoFactor> {
        self.authenticator.as_ref().and(self.two_factor.as_ref())
    }

    /// The auth mail, or `None` when the panel mails nothing.
    pub(crate) fn auth_mail(&self) -> Option<&AuthMail> {
        self.authenticator.as_ref().and(self.mail.as_ref())
    }

    /// The features that send mail, by name, for the mount's checks.
    pub(super) fn mailing(&self) -> Vec<&'static str> {
        [
            self.reset.is_some().then_some("password_reset"),
            self.verification.is_some().then_some("email_verification"),
            self.two_factor
                .as_ref()
                .is_some_and(|two_factor| two_factor.mails())
                .then_some("two_factor"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Whether the panel issues [`AuthToken`](super::AuthToken)s, so the model must be registered:
    /// it mails links, or challenges a second factor.
    pub(crate) fn issues_tokens(&self) -> bool {
        self.authenticator.is_some() && (!self.mailing().is_empty() || self.two_factor.is_some())
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
                .field("mail", &self.mail)
                .finish_non_exhaustive()
        }
    }
}
