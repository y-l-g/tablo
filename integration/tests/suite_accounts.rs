//! The integration suite's accounts binary: sign-in, sign-up, verification, two-factor, password
//! reset and profiles, the mail and notifications they send, and the locale and time zone a session
//! carries.
//!
//! Each module is one suite file; `common` holds the fixtures every binary shares. Filter per file
//! with `cargo test -p tablo-integration <module>::`.

#[allow(
    dead_code,
    unused_imports,
    reason = "each binary uses part of the shared fixtures"
)]
mod common;

mod account;
mod admin_credentials;
mod auth_events;
mod auth_override;
mod email_change;
mod external;
mod impersonation;
mod locale;
mod login;
mod mail;
mod notifications;
mod oidc;
mod password_reset;
mod profile;
mod registration;
mod session;
mod stream_pool;
mod tenant_profile;
mod tenant_registration;
mod time_zone;
mod two_factor;
mod two_factor_email;
mod two_factor_shared;
mod verification;
