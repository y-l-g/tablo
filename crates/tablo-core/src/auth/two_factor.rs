//! Two-factor sign-in: the [`TwoFactor`] a panel installs, the [`AuthFactor`] rows that hold each
//! account's authenticator secret and recovery codes, and the checks the challenge and settings
//! pages run against them.

mod challenge;
mod cipher;
mod pages;
mod settings;
mod totp;

use std::time::Duration;

use jiff::Timestamp;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use topcoat::{
    context::{Cx, try_app_context},
    cookie::Key,
};

pub(crate) use self::{challenge::challenge_url, settings::settings_url};
pub(super) use self::{
    challenge::{begin as begin_challenge, challenge_page, challenge_post},
    settings::{settings_page, settings_post},
};
use super::{LoginThrottle, PanelUser, infrastructure_failure};
use crate::panel::{panel_prefix, state::PanelState};

/// A panel's two-factor sign-in, installed with [`Auth::two_factor`](super::Auth::two_factor):
/// each signed-in user may add an authenticator app at `{prefix}/two-factor`, and an account that
/// has one signs in with a code from it after its password.
///
/// ```rust
/// use tablo_core::{Auth, TwoFactor};
///
/// let auth = Auth::password().two_factor(TwoFactor::new().required());
/// # let _ = auth;
/// ```
#[derive(Debug)]
pub struct TwoFactor {
    required: bool,
    throttle: LoginThrottle,
}

impl TwoFactor {
    /// Two-factor sign-in each user may turn on.
    #[must_use]
    pub fn new() -> Self {
        Self {
            required: false,
            throttle: LoginThrottle::new(5, Duration::from_mins(1)),
        }
    }

    /// Sends a signed-in user without an authenticator to `{prefix}/two-factor` before anything
    /// else: a page answers with a redirect to it and any other request with 403, after an
    /// unverified address.
    #[must_use]
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// How many codes one account may try in a window, from any client, at sign-in and on the
    /// settings page together; defaults to five a minute.
    #[must_use]
    pub fn throttle(mut self, throttle: LoginThrottle) -> Self {
        self.throttle = throttle;
        self
    }
}

impl Default for TwoFactor {
    fn default() -> Self {
        Self::new()
    }
}

/// Shipped second-factor record: an account's authenticator secret, sealed, or the hash of one of
/// its recovery codes, on the panel that enrolled it.
#[derive(Debug, Clone, toasty::Model)]
pub struct AuthFactor {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    /// [`PanelUser::user_id`](super::PanelUser::user_id) of the account the factor signs in.
    #[index]
    pub user_id: String,
    /// The prefix of the panel that enrolled the factor: the only panel it signs in to.
    pub panel: String,
    pub kind: FactorKind,
    /// The authenticator secret, sealed under the router's cookie key, or the hex SHA-256 of a
    /// recovery code.
    pub secret: String,
    /// When a code proved the authenticator app holds the secret; `None` while it is being set
    /// up. A recovery code is confirmed when issued.
    pub confirmed_at: Option<Timestamp>,
    /// The last 30-second step an authenticator code signed in with: a code works once.
    pub last_step: Option<i64>,
    pub created_at: Timestamp,
}

/// What an [`AuthFactor`] is. Stored as its `snake_case` name in a text column, so a kind a later
/// release adds needs no migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
#[non_exhaustive]
pub enum FactorKind {
    /// An authenticator app's RFC 6238 secret.
    Authenticator,
    /// A single-use recovery code, for an account that lost its authenticator.
    RecoveryCode,
}

/// How many recovery codes a set-up or a regeneration issues.
const RECOVERY_CODES: usize = 8;

/// The characters a recovery code draws from: lowercase base32, which reads without ambiguity.
const RECOVERY_ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// The router's cookie key, which seals authenticator secrets.
fn key(cx: &Cx) -> topcoat::Result<&Key> {
    try_app_context::<Key>(cx)
        .ok_or_else(|| crate::error::declaration("two-factor sign-in needs the router's `Key`"))
}

/// What a sealed secret is bound to: its account on its panel.
fn binding(user_id: &str, panel: &str) -> String {
    format!("{user_id}\n{panel}")
}

/// Every factor `user_id` holds on the panel at `panel`, read through `ex`.
async fn factors(
    ex: &mut dyn toasty::Executor,
    panel: &str,
    user_id: &str,
) -> topcoat::Result<Vec<AuthFactor>> {
    AuthFactor::filter(
        AuthFactor::fields()
            .user_id()
            .eq(user_id.to_string())
            .and(AuthFactor::fields().panel().eq(panel.to_string())),
    )
    .exec(ex)
    .await
    .map_err(infrastructure_failure)
}

/// What `user_id` holds on the request's panel: a confirmed authenticator, an unconfirmed one,
/// and how many recovery codes are left.
#[derive(Debug, Default)]
pub(super) struct Enrolment {
    pub(super) confirmed: Option<AuthFactor>,
    pub(super) unconfirmed: Option<AuthFactor>,
    pub(super) recovery_codes: usize,
}

/// `user_id`'s [`Enrolment`] on the request's panel.
pub(super) async fn enrolment(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    user_id: &str,
) -> topcoat::Result<Enrolment> {
    enrolment_on(ex, &panel_prefix(cx), user_id).await
}

/// `user_id`'s [`Enrolment`] on the panel at `panel`.
async fn enrolment_on(
    ex: &mut dyn toasty::Executor,
    panel: &str,
    user_id: &str,
) -> topcoat::Result<Enrolment> {
    let mut enrolment = Enrolment::default();
    for factor in factors(ex, panel, user_id).await? {
        match (factor.kind, factor.confirmed_at.is_some()) {
            (FactorKind::Authenticator, true) => enrolment.confirmed = Some(factor),
            (FactorKind::Authenticator, false) => enrolment.unconfirmed = Some(factor),
            (FactorKind::RecoveryCode, _) => enrolment.recovery_codes += 1,
        }
    }
    Ok(enrolment)
}

/// Whether `user` signs in to the request's panel with a second factor.
pub(super) async fn enrolled(cx: &Cx, user: &dyn PanelUser) -> topcoat::Result<bool> {
    let mut db = crate::db::db(cx);
    Ok(enrolment(cx, &mut db, &user.user_id())
        .await?
        .confirmed
        .is_some())
}

/// Whether the request's panel offers the signed-in user two-factor sign-in.
pub(crate) fn offered(cx: &Cx) -> bool {
    super::signed(cx).is_some_and(|signed| signed.panel.auth.two_factor_config().is_some())
}

/// Whether `panel` sends `user` to set up an authenticator before anything else: it requires one,
/// and the user holds none.
pub(super) async fn setup_pending(
    cx: &Cx,
    panel: &PanelState,
    user: &dyn PanelUser,
) -> topcoat::Result<bool> {
    match panel.auth.two_factor_config() {
        Some(two_factor) if two_factor.required => {
            let mut db = crate::db::db(cx);
            let enrolment = enrolment_on(&mut db, &panel.prefix, &user.user_id()).await?;
            Ok(enrolment.confirmed.is_none())
        }
        _ => Ok(false),
    }
}

/// Starts setting up an authenticator for `user_id`: a new secret, unconfirmed, in place of any
/// earlier unconfirmed one. Answers the secret, for the QR code.
async fn start(cx: &Cx, ex: &mut dyn toasty::Executor, user_id: &str) -> topcoat::Result<Vec<u8>> {
    let panel = panel_prefix(cx);
    AuthFactor::filter(
        AuthFactor::fields()
            .user_id()
            .eq(user_id.to_string())
            .and(AuthFactor::fields().panel().eq(panel.clone()))
            .and(AuthFactor::fields().kind().eq(FactorKind::Authenticator))
            .and(AuthFactor::fields().confirmed_at().is_none()),
    )
    .delete()
    .exec(&mut *ex)
    .await
    .map_err(infrastructure_failure)?;
    let secret = totp::secret();
    let sealed = cipher::seal(key(cx)?, &secret, &binding(user_id, &panel));
    toasty::create!(AuthFactor {
        user_id: user_id.to_string(),
        panel,
        kind: FactorKind::Authenticator,
        secret: sealed,
        confirmed_at: None,
        last_step: None,
        created_at: Timestamp::now(),
    })
    .exec(ex)
    .await
    .map_err(infrastructure_failure)?;
    Ok(secret.to_vec())
}

/// The secret `factor` seals, or `None` when the router's key no longer opens it.
fn secret(cx: &Cx, factor: &AuthFactor) -> topcoat::Result<Option<Vec<u8>>> {
    Ok(cipher::open(
        key(cx)?,
        &factor.secret,
        &binding(&factor.user_id, &factor.panel),
    ))
}

/// Whether `code` is the current code of `factor`'s authenticator, not used before; a match
/// records its step through `ex`, so the code works once.
async fn accept_code(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    factor: &AuthFactor,
    code: &str,
) -> topcoat::Result<bool> {
    let Some(secret) = secret(cx, factor)? else {
        tracing::error!(panel = %factor.panel, "a two-factor secret does not open under the key");
        return Ok(false);
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| crate::error::declaration("the clock is before 1970"))?
        .as_secs();
    let used = factor.last_step.and_then(|step| u64::try_from(step).ok());
    let Some(step) = totp::verify(&secret, code, totp::step(now), used) else {
        return Ok(false);
    };
    AuthFactor::filter(AuthFactor::fields().id().eq(factor.id))
        .update()
        .last_step(i64::try_from(step).ok())
        .exec(ex)
        .await
        .map_err(infrastructure_failure)?;
    Ok(true)
}

/// A recovery code as stored: the hex SHA-256 of its characters, lowercase, without separators.
fn recovery_hash(code: &str) -> String {
    let normalized: String = code
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let digest = Sha256::digest(normalized.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Whether `code` is one of `user_id`'s recovery codes on the request's panel; a match is deleted
/// through `ex`, so each works once.
async fn spend_recovery_code(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    user_id: &str,
    code: &str,
) -> topcoat::Result<bool> {
    let hash = recovery_hash(code);
    let codes = factors(&mut *ex, &panel_prefix(cx), user_id).await?;
    let Some(matched) = codes.iter().find(|factor| {
        factor.kind == FactorKind::RecoveryCode
            && bool::from(factor.secret.as_bytes().ct_eq(hash.as_bytes()))
    }) else {
        return Ok(false);
    };
    AuthFactor::filter(AuthFactor::fields().id().eq(matched.id))
        .delete()
        .exec(ex)
        .await
        .map_err(infrastructure_failure)?;
    Ok(true)
}

/// Whether `code` proves `user_id` holds their second factor on the request's panel: the current
/// code of their confirmed authenticator, or one of their recovery codes, which it uses up.
pub(super) async fn accept(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    user_id: &str,
    code: &str,
) -> topcoat::Result<bool> {
    let enrolment = enrolment(cx, &mut *ex, user_id).await?;
    let Some(factor) = enrolment.confirmed else {
        return Ok(false);
    };
    if code.trim().bytes().all(|b| b.is_ascii_digit()) {
        accept_code(cx, ex, &factor, code).await
    } else {
        spend_recovery_code(cx, ex, user_id, code).await
    }
}

/// Fresh recovery codes for `user_id` on the request's panel, in place of any earlier ones.
/// Answers the codes, which are shown once: only their hashes are stored.
async fn issue_recovery_codes(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    user_id: &str,
) -> topcoat::Result<Vec<String>> {
    let panel = panel_prefix(cx);
    AuthFactor::filter(
        AuthFactor::fields()
            .user_id()
            .eq(user_id.to_string())
            .and(AuthFactor::fields().panel().eq(panel.clone()))
            .and(AuthFactor::fields().kind().eq(FactorKind::RecoveryCode)),
    )
    .delete()
    .exec(&mut *ex)
    .await
    .map_err(infrastructure_failure)?;
    let mut codes = Vec::with_capacity(RECOVERY_CODES);
    let now = Timestamp::now();
    for _ in 0..RECOVERY_CODES {
        let code = recovery_code();
        toasty::create!(AuthFactor {
            user_id: user_id.to_string(),
            panel: panel.clone(),
            kind: FactorKind::RecoveryCode,
            secret: recovery_hash(&code),
            confirmed_at: Some(now),
            last_step: None,
            created_at: now,
        })
        .exec(&mut *ex)
        .await
        .map_err(infrastructure_failure)?;
        codes.push(code);
    }
    Ok(codes)
}

/// A random recovery code: ten characters of [`RECOVERY_ALPHABET`], in two groups of five.
fn recovery_code() -> String {
    let mut bytes = [0u8; 10];
    getrandom::fill(&mut bytes).expect("the recovery code randomness source failed");
    // 256 is a multiple of 32, so the low five bits are uniform.
    let chars: String = bytes
        .iter()
        .map(|byte| char::from(RECOVERY_ALPHABET[usize::from(byte & 31)]))
        .collect();
    format!("{}-{}", &chars[..5], &chars[5..])
}

/// Deletes every factor `user_id` holds on the request's panel, through `ex`.
async fn remove_all(cx: &Cx, ex: &mut dyn toasty::Executor, user_id: &str) -> topcoat::Result<()> {
    AuthFactor::filter(
        AuthFactor::fields()
            .user_id()
            .eq(user_id.to_string())
            .and(AuthFactor::fields().panel().eq(panel_prefix(cx))),
    )
    .delete()
    .exec(ex)
    .await
    .map_err(infrastructure_failure)
}

/// The mistakes in a panel's [`Auth::two_factor`](super::Auth::two_factor): two-factor sign-in
/// without auth, and a router without the cookie key that seals the secrets.
pub(super) fn declaration_errors(
    auth: &super::Auth,
    has_key: bool,
) -> Vec<crate::DeclarationErrorKind> {
    use crate::{DeclarationErrorKind, declaration::TwoFactorFault};

    if auth.two_factor.is_none() {
        return Vec::new();
    }
    if auth.authenticator().is_none() {
        return vec![DeclarationErrorKind::TwoFactor(
            TwoFactorFault::AuthDisabled,
        )];
    }
    if has_key {
        Vec::new()
    } else {
        vec![DeclarationErrorKind::TwoFactor(TwoFactorFault::MissingKey)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recovery code reads the same however it is typed, and two codes differ.
    #[test]
    fn a_recovery_code_matches_however_it_is_typed() {
        let code = recovery_code();
        assert_eq!(code.len(), 11);
        assert_eq!(recovery_hash(&code), recovery_hash(&code.to_uppercase()));
        assert_eq!(recovery_hash(&code), recovery_hash(&code.replace('-', " ")));
        assert_ne!(recovery_hash(&code), recovery_hash(&recovery_code()));
    }
}
