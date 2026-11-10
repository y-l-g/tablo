//! The declaration mistakes of a panel's auth: its sign-up, profile, password-reset,
//! email-verification, tenant registration and tenant settings pages, and the mail they send.

use std::fmt;

/// Why a panel's sign-up page cannot be served:
/// [`DeclarationErrorKind::SignUp`](super::DeclarationErrorKind::SignUp).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SignUpFault {
    /// `Auth::registration` on `Auth::disabled`, which serves no sign-in to land on.
    ///
    /// Turn authentication on.
    AuthDisabled,
    /// A field named like a key the sign-up POST carries itself: `csrf_token` or `next`.
    ///
    /// Rename the field.
    ReservedField(String),
    /// A file field, whose upload the sign-up POST does not read.
    ///
    /// Ask for the file once the account signs in.
    FileField(String),
    /// A relationship choice, whose options would list records to a visitor nobody signed in.
    ///
    /// Offer static options.
    RelationshipField(String),
}

/// Why a panel's profile page cannot be served:
/// [`DeclarationErrorKind::Profile`](super::DeclarationErrorKind::Profile).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProfileFault {
    /// `Auth::profile` on `Auth::disabled`, which signs nobody in to edit.
    ///
    /// Turn authentication on.
    AuthDisabled,
    /// A field named like a key the profile POST carries itself: `csrf_token`.
    ///
    /// Rename the field.
    ReservedField(String),
    /// A file field, whose upload the profile POST does not read.
    ///
    /// Edit the file on a resource's form.
    FileField(String),
    /// A relationship choice, whose options no route serves the profile page.
    ///
    /// Offer static options.
    RelationshipField(String),
}

/// Why a panel's password reset cannot be served:
/// [`DeclarationErrorKind::PasswordReset`](super::DeclarationErrorKind::PasswordReset).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResetFault {
    /// `Auth::password_reset` on `Auth::disabled`, which signs nobody in after a reset.
    ///
    /// Turn authentication on.
    AuthDisabled,
}

impl fmt::Display for ResetFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => f.write_str(
                "a password reset needs auth: `Auth::disabled()` serves no sign-in to reset",
            ),
        }
    }
}

impl fmt::Display for ProfileFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => {
                f.write_str("a profile page needs auth: `Auth::disabled()` signs nobody in to edit")
            }
            Self::ReservedField(field) => write!(
                f,
                "the profile form declares a field named '{field}', which its POST carries \
             itself: rename the field"
            ),
            Self::FileField(field) => write!(
                f,
                "the profile form declares a file field '{field}', which its POST does not \
             upload: edit the file on a resource's form"
            ),
            Self::RelationshipField(field) => write!(
                f,
                "the profile form declares a relationship choice '{field}', whose options no \
             route serves the profile page: offer static options"
            ),
        }
    }
}

impl fmt::Display for SignUpFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => f.write_str(
                "registration needs auth: `Auth::disabled()` serves no sign-in for a new \
             account to land on",
            ),
            Self::ReservedField(field) => write!(
                f,
                "the sign-up form declares a field named '{field}', which its POST carries \
             itself: rename the field"
            ),
            Self::FileField(field) => write!(
                f,
                "the sign-up form declares a file field '{field}', which its POST does not \
             upload: ask for the file once the account signs in"
            ),
            Self::RelationshipField(field) => write!(
                f,
                "the sign-up form declares a relationship choice '{field}', which would list \
             records to a visitor nobody signed in: offer static options"
            ),
        }
    }
}

/// Why a panel's auth mail cannot be sent:
/// [`DeclarationErrorKind::AuthMail`](super::DeclarationErrorKind::AuthMail).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthMailFault {
    /// A feature that mails links or codes, named as its `Auth` method, on an `Auth` without
    /// `Auth::mail`.
    ///
    /// Install a sender with `Auth::mail(AuthMail::new(..))`.
    Unset {
        /// The `Auth` method that installs the feature, such as `password_reset`.
        feature: &'static str,
    },
    /// The base URL links start with is not an absolute `http` or `https` URL.
    ///
    /// Pass the scheme and host, such as `https://admin.example.com`.
    BaseUrl(String),
    /// The router registers no `Mailer` to send the mail through.
    ///
    /// Add one with `.with(Mailer::new(..))` before `.panel(..)`.
    MissingMail,
}

impl fmt::Display for AuthMailFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unset { feature } => write!(
                f,
                "`Auth::{feature}` sends mail, but the auth sets no sender: install one with \
             `Auth::mail(AuthMail::new(..))`"
            ),
            Self::BaseUrl(url) => write!(
                f,
                "the auth mail's base URL '{url}' is not an absolute http or https URL: pass the \
             scheme and host the app answers on, such as `https://admin.example.com`"
            ),
            Self::MissingMail => f.write_str(
                "the panel's auth mails links, but the router registers no `Mailer`: add \
             one with `.with(Mailer::new(..))` before mounting the panel",
            ),
        }
    }
}

/// Why a panel's email verification cannot be served:
/// [`DeclarationErrorKind::EmailVerification`](super::DeclarationErrorKind::EmailVerification).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum VerificationFault {
    /// `Auth::email_verification` on `Auth::disabled`, which signs nobody in to verify.
    ///
    /// Turn authentication on.
    AuthDisabled,
}

impl fmt::Display for VerificationFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => f.write_str(
                "email verification needs auth: `Auth::disabled()` signs nobody in to verify",
            ),
        }
    }
}

/// Why a panel's tenant registration page cannot be served:
/// [`DeclarationErrorKind::TenantRegistration`](super::DeclarationErrorKind::TenantRegistration).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TenantRegistrationFault {
    /// `Auth::tenant_registration` on `Auth::disabled`, which signs nobody in to register one.
    ///
    /// Turn authentication on.
    AuthDisabled,
    /// A field named like a key the registration POST carries itself: `csrf_token`.
    ///
    /// Rename the field.
    ReservedField(String),
    /// A file field, whose upload the registration POST does not read.
    ///
    /// Ask for the file on a resource's form.
    FileField(String),
    /// A relationship choice, whose options no route serves the registration page.
    ///
    /// Offer static options.
    RelationshipField(String),
}

impl fmt::Display for TenantRegistrationFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => f.write_str(
                "tenant registration needs auth: `Auth::disabled()` signs nobody in to register \
             a tenant",
            ),
            Self::ReservedField(field) => write!(
                f,
                "the tenant registration form declares a field named '{field}', which its POST \
             carries itself: rename the field"
            ),
            Self::FileField(field) => write!(
                f,
                "the tenant registration form declares a file field '{field}', which its POST \
             does not upload: ask for the file on a resource's form"
            ),
            Self::RelationshipField(field) => write!(
                f,
                "the tenant registration form declares a relationship choice '{field}', whose \
             options no route serves the registration page: offer static options"
            ),
        }
    }
}

/// Why a panel's tenant settings page cannot be served:
/// [`DeclarationErrorKind::TenantProfile`](super::DeclarationErrorKind::TenantProfile).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TenantProfileFault {
    /// `Auth::tenant_profile` on `Auth::disabled`, which signs nobody in to edit a tenant.
    ///
    /// Turn authentication on.
    AuthDisabled,
    /// A field named like a key the settings POST carries itself: `csrf_token`.
    ///
    /// Rename the field.
    ReservedField(String),
    /// A file field, whose upload the settings POST does not read.
    ///
    /// Edit the file on a resource's form.
    FileField(String),
    /// A relationship choice, whose options no route serves the settings page.
    ///
    /// Offer static options.
    RelationshipField(String),
}

impl fmt::Display for TenantProfileFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => f.write_str(
                "a tenant settings page needs auth: `Auth::disabled()` signs nobody in to edit a \
             tenant",
            ),
            Self::ReservedField(field) => write!(
                f,
                "the tenant settings form declares a field named '{field}', which its POST \
             carries itself: rename the field"
            ),
            Self::FileField(field) => write!(
                f,
                "the tenant settings form declares a file field '{field}', which its POST does \
             not upload: edit the file on a resource's form"
            ),
            Self::RelationshipField(field) => write!(
                f,
                "the tenant settings form declares a relationship choice '{field}', whose \
             options no route serves the settings page: offer static options"
            ),
        }
    }
}

/// Why the panel's two-factor sign-in cannot be served:
/// [`DeclarationErrorKind::TwoFactor`](super::DeclarationErrorKind::TwoFactor).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TwoFactorFault {
    /// `Auth::two_factor` on `Auth::disabled`, which signs nobody in.
    ///
    /// Turn authentication on.
    AuthDisabled,
    /// The router registers no cookie `Key`, which seals the authenticator
    /// secrets.
    ///
    /// Call `.with(Key::from(..))` before `.panel(..)`.
    MissingKey,
}

impl fmt::Display for TwoFactorFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => f.write_str(
                "two-factor sign-in needs auth: `Auth::disabled()` signs nobody in to ask a \
             second factor of",
            ),
            Self::MissingKey => f.write_str(
                "two-factor sign-in seals each authenticator secret under the router's cookie \
             key, and the router has none: register a `topcoat::cookie::Key` with `.with(..)`",
            ),
        }
    }
}

/// Why the panel's provider sign-in cannot be served:
/// [`DeclarationErrorKind::Oidc`](super::DeclarationErrorKind::Oidc).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OidcFault {
    /// `Auth::oidc` on `Auth::disabled`, which signs nobody in.
    ///
    /// Turn authentication on.
    AuthDisabled,
    /// `Auth::oidc` on `Auth::external`, whose app signs every user in itself.
    ///
    /// Sign in through the provider in the app, or use `Auth::custom`.
    ExternalAuth,
    /// The router registers no cookie `Key`, which seals a sign-in's secrets while the browser
    /// is at the provider.
    ///
    /// Call `.with(Key::from(..))` before `.panel(..)`.
    MissingKey,
    /// `Oidc::new`'s base URL is not an absolute `https` origin.
    ///
    /// Pass the app's public origin, such as `https://app.example.com`.
    BaseUrl(String),
    /// `Oidc` without a provider.
    ///
    /// Add one with `Oidc::provider`.
    NoProvider,
    /// A provider key that is not lowercase ASCII letters, digits and `-`.
    ///
    /// Rename the provider.
    ProviderKey(String),
    /// Two providers with one key.
    ///
    /// Give each a distinct key.
    DuplicateProvider(String),
    /// A provider whose issuer is not an `https` URL, nor an `http` one on a loopback host.
    ///
    /// Fix the issuer.
    Issuer(String),
    /// A provider's `param` names an authorization parameter the panel sets: the provider key,
    /// then the parameter.
    ///
    /// Drop the parameter.
    ReservedParam(String, String),
}

impl fmt::Display for OidcFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => f.write_str(
                "provider sign-in needs auth: `Auth::disabled()` signs nobody in to link an \
                 identity to",
            ),
            Self::ExternalAuth => f.write_str(
                "provider sign-in cannot sit beside `Auth::external`: the app signs every user in \
                 itself, so sign in through the provider there",
            ),
            Self::MissingKey => f.write_str(
                "provider sign-in seals each pending sign-in under the router's cookie key, and \
                 the router has none: register a `topcoat::cookie::Key` with `.with(..)`",
            ),
            Self::BaseUrl(url) => write!(
                f,
                "provider sign-in's base URL '{url}' is not an absolute `https` origin: pass the \
                 app's public origin, such as `https://app.example.com`"
            ),
            Self::NoProvider => {
                f.write_str("provider sign-in has no provider: add one with `Oidc::provider`")
            }
            Self::ProviderKey(key) => write!(
                f,
                "provider key '{key}' cannot name a route: use lowercase ASCII letters, digits \
                 and `-`"
            ),
            Self::DuplicateProvider(key) => write!(
                f,
                "two providers have the key '{key}': give each a distinct key"
            ),
            Self::Issuer(key) => write!(
                f,
                "provider '{key}' has an issuer that is not an `https` URL, nor an `http` one on \
                 a loopback host"
            ),
            Self::ReservedParam(key, name) => write!(
                f,
                "provider '{key}' sets the authorization parameter '{name}', which the panel \
                 sets itself: drop it"
            ),
        }
    }
}
