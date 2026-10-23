//! Codes mailed as a second factor: six digits, one live per account, for ten minutes, held as an
//! [`AuthToken`](crate::auth::AuthToken) whose hash binds the code to its account and panel.

use std::time::Duration;

use topcoat::{context::Cx, router::error::too_many_requests};

use super::{TwoFactor, binding, cipher, key};
use crate::auth::{
    PanelUser,
    realm::{current_realm, hosted, panel_prefix},
    token::{self, TokenPurpose},
};

/// How long a mailed code works.
const CODE_LIFETIME: Duration = Duration::from_mins(10);

/// A random six-digit code, uniform over all million.
fn code() -> String {
    // The largest multiple of a million a `u32` holds: values past it would favour low codes.
    const LIMIT: u32 = u32::MAX - u32::MAX % 1_000_000;
    loop {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).expect("the mailed code randomness source failed");
        let value = u32::from_be_bytes(bytes);
        if value < LIMIT {
            return format!("{:06}", value % 1_000_000);
        }
    }
}

/// `code` as its token stores it, for `user_id` on the request's panel.
fn stored(cx: &Cx, user_id: &str, code: &str) -> topcoat::Result<String> {
    Ok(cipher::code_hash(
        key(cx)?,
        &binding(user_id, &panel_prefix(cx)),
        code,
    ))
}

/// What [`send`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sent {
    /// Mailed a new code, which replaced any earlier one.
    Mailed,
    /// Mailed nothing: the account had as many codes as the throttle allows, and the last one
    /// still works.
    Throttled,
    /// Mailed nothing: the user stores no address, or the panel no longer mails codes.
    Unavailable,
}

impl Sent {
    /// `Ok` unless the throttle held the code back: then the 429 a request asking for one answers.
    pub(super) fn or_too_many(self, two_factor: &TwoFactor) -> topcoat::Result<()> {
        match self {
            Self::Throttled => {
                Err(too_many_requests(two_factor.mail_throttle.window_secs()).into())
            }
            Self::Mailed | Self::Unavailable => Ok(()),
        }
    }
}

/// Mails `user` a new code, in place of any earlier one, unless the account had as many as the
/// throttle allows. A panel that stopped mailing codes mails none: the account's recovery codes
/// still sign in.
pub(super) async fn send(
    cx: &Cx,
    two_factor: &TwoFactor,
    user: &dyn PanelUser,
) -> topcoat::Result<Sent> {
    let mail = current_realm(cx).and_then(|panel| panel.auth.auth_mail());
    let (Some(address), Some(mail), true) = (user.email(), mail, two_factor.mails()) else {
        return Ok(Sent::Unavailable);
    };
    let user_id = user.user_id();
    if !two_factor.mail_throttle.attempt(&user_id, None) {
        return Ok(Sent::Throttled);
    }
    let code = code();
    let mut db = crate::db::db(cx);
    token::record(
        cx,
        &mut db,
        TokenPurpose::TwoFactorEmailCode,
        &user_id,
        None,
        CODE_LIFETIME,
        stored(cx, &user_id, &code)?,
    )
    .await?;
    let brand = hosted(cx)?.brand_name(cx);
    let message = mail.compose(
        address,
        crate::t!(cx, "tablo-two-factor-mail-subject", brand = brand.clone()),
        crate::t!(
            cx,
            "tablo-two-factor-mail-body",
            brand = brand,
            code = code.clone()
        ),
    )?;
    // The code is recorded: a mail server that fails leaves the page up, and its button to send
    // another, rather than a 500 after the password verified.
    if let Err(error) = topcoat::mail::send(cx, message).await {
        tracing::error!(error = %error, "two-factor code not sent");
    }
    Ok(Sent::Mailed)
}

/// Whether `code` is the code last mailed to `user_id` on the request's panel, still live; a
/// match is spent through `ex`, so it works once.
pub(super) async fn accept(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    user_id: &str,
    code: &str,
) -> topcoat::Result<bool> {
    let hash = stored(cx, user_id, code)?;
    let Some(row) = token::recorded(cx, &mut *ex, TokenPurpose::TwoFactorEmailCode, hash).await?
    else {
        return Ok(false);
    };
    if row.user_id != user_id {
        return Ok(false);
    }
    token::spend(ex, &row).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A code is six digits.
    #[test]
    fn a_code_is_six_digits() {
        for _ in 0..100 {
            let code = code();
            assert_eq!(code.len(), 6);
            assert!(code.bytes().all(|b| b.is_ascii_digit()));
        }
    }
}
