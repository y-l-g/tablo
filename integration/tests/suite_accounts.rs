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

mod admin_credentials;
mod auth_override;
mod email_change;
mod locale;
mod login;
mod mail;
mod notifications;
mod password_reset;
mod profile;
mod registration;
mod stream_pool;
mod tenant_profile;
mod tenant_registration;
mod time_zone;
mod two_factor;
mod two_factor_email;
mod verification;
