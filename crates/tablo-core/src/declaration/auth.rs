//! The declaration mistakes of a panel's auth: its sign-up, profile, password-reset,
//! email-verification, tenant registration and tenant settings pages, and the mail they send.

use std::fmt;

/// Why a panel's sign-up page cannot be served:
/// [`DeclarationErrorKind::SignUp`](super::DeclarationErrorKind::SignUp).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SignUpFault {
    /// `Auth::registration` on `Auth::disabled`, which serves no sign-in to land on.
    AuthDisabled,
    /// The registrar creates another user type than the authenticator loads.
    UserType {
        /// The registrar's `User` type.
        registrar: &'static str,
    },
    /// A field named like a key the sign-up POST carries itself: `csrf_token` or `next`.
    ReservedField(String),
    /// A file field, whose upload the sign-up POST does not read.
    FileField(String),
    /// A relationship choice, whose options would list records to a visitor nobody signed in.
    RelationshipField(String),
}

/// Why a panel's profile page cannot be served:
/// [`DeclarationErrorKind::Profile`](super::DeclarationErrorKind::Profile).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProfileFault {
    /// `Auth::profile` on `Auth::disabled`, which signs nobody in to edit.
    AuthDisabled,
    /// The editor edits another user type than the authenticator loads.
    UserType {
        /// The editor's `User` type.
        editor: &'static str,
    },
    /// A field named like a key the profile POST carries itself: `csrf_token`.
    ReservedField(String),
    /// A file field, whose upload the profile POST does not read.
    FileField(String),
    /// A relationship choice, whose options no route serves the profile page.
    RelationshipField(String),
}

/// Why a panel's password reset cannot be served:
/// [`DeclarationErrorKind::PasswordReset`](super::DeclarationErrorKind::PasswordReset).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResetFault {
    /// `Auth::password_reset` on `Auth::disabled`, which signs nobody in after a reset.
    AuthDisabled,
    /// The resetter resets another user type than the authenticator loads.
    UserType {
        /// The resetter's `User` type.
        resetter: &'static str,
    },
}

impl fmt::Display for ResetFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => f.write_str(
                "a password reset needs auth: `Auth::disabled()` serves no sign-in to reset",
            ),
            Self::UserType { resetter } => write!(
                f,
                "the password resetter resets `{resetter}`, but the authenticator loads another \
             type: reset the type the panel signs in"
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
            Self::UserType { editor } => write!(
                f,
                "the profile editor edits `{editor}`, but the authenticator loads another \
             type: edit the type the panel signs in"
            ),
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
            Self::UserType { registrar } => write!(
                f,
                "the registrar creates `{registrar}`, but the authenticator loads another \
             type: register the type the panel signs in"
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
    Unset {
        /// The `Auth` method that installs the feature, such as `password_reset`.
        feature: &'static str,
    },
    /// The base URL links start with is not an absolute `http` or `https` URL.
    BaseUrl(String),
    /// The router registers no `MailConfig` to send the mail through.
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
                "the panel's auth mails links, but the router registers no `MailConfig`: install \
             one with `.mail(..)` before mounting the panel",
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
    AuthDisabled,
    /// The verifier verifies another user type than the authenticator loads.
    UserType {
        /// The verifier's `User` type.
        verifier: &'static str,
    },
}

impl fmt::Display for VerificationFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => f.write_str(
                "email verification needs auth: `Auth::disabled()` signs nobody in to verify",
            ),
            Self::UserType { verifier } => write!(
                f,
                "the email verifier verifies `{verifier}`, but the authenticator loads another \
             type: verify the type the panel signs in"
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
    AuthDisabled,
    /// The registrar registers tenants for another user type than the authenticator loads.
    UserType {
        /// The registrar's `User` type.
        registrar: &'static str,
    },
    /// A field named like a key the registration POST carries itself: `csrf_token`.
    ReservedField(String),
    /// A file field, whose upload the registration POST does not read.
    FileField(String),
    /// A relationship choice, whose options no route serves the registration page.
    RelationshipField(String),
}

impl fmt::Display for TenantRegistrationFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => f.write_str(
                "tenant registration needs auth: `Auth::disabled()` signs nobody in to register \
             a tenant",
            ),
            Self::UserType { registrar } => write!(
                f,
                "the tenant registrar registers tenants for `{registrar}`, but the authenticator \
             loads another type: register for the type the panel signs in"
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
    AuthDisabled,
    /// The editor edits tenants for another user type than the authenticator loads.
    UserType {
        /// The editor's `User` type.
        editor: &'static str,
    },
    /// A field named like a key the settings POST carries itself: `csrf_token`.
    ReservedField(String),
    /// A file field, whose upload the settings POST does not read.
    FileField(String),
    /// A relationship choice, whose options no route serves the settings page.
    RelationshipField(String),
}

impl fmt::Display for TenantProfileFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthDisabled => f.write_str(
                "a tenant settings page needs auth: `Auth::disabled()` signs nobody in to edit a \
             tenant",
            ),
            Self::UserType { editor } => write!(
                f,
                "the tenant editor edits tenants for `{editor}`, but the authenticator loads \
             another type: edit for the type the panel signs in"
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
    AuthDisabled,
    /// The router registers no cookie `Key` as app context, which seals the authenticator
    /// secrets.
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
             key, and the router has none: register a `topcoat::cookie::Key` as app context",
            ),
        }
    }
}
