//! RFC 6238 time-based one-time codes, as authenticator apps compute them: HMAC-SHA1 over the
//! 30-second step count, six digits.

use hmac::{Hmac, Mac};
use sha1::Sha1;
use subtle::ConstantTimeEq;

/// The bytes a new secret holds: the 160 bits RFC 4226 recommends for HMAC-SHA1.
pub(super) const SECRET_BYTES: usize = 20;

/// How long one code lasts, in seconds.
const STEP_SECS: u64 = 30;

/// The digits a code holds.
const DIGITS: u32 = 6;

/// How many steps either side of now a code may come from, for a phone's clock that drifts.
const SKEW: u64 = 1;

/// A fresh random secret.
pub(super) fn secret() -> [u8; SECRET_BYTES] {
    let mut secret = [0; SECRET_BYTES];
    getrandom::fill(&mut secret).expect("the two-factor secret randomness source failed");
    secret
}

/// `secret` as an authenticator app reads it: unpadded base32.
pub(super) fn encode(secret: &[u8]) -> String {
    data_encoding::BASE32_NOPAD.encode(secret)
}

/// The `otpauth://` URI a set-up QR code holds: `issuer` and `account` label the entry the app
/// adds.
pub(super) fn uri(secret: &[u8], issuer: &str, account: &str) -> String {
    let label: String =
        percent_encoding::utf8_percent_encode(&format!("{issuer}:{account}"), URI_COMPONENT)
            .collect();
    let mut query = form_urlencoded::Serializer::new(String::new());
    query
        .append_pair("secret", &encode(secret))
        .append_pair("issuer", issuer)
        .append_pair("algorithm", "SHA1")
        .append_pair("digits", &DIGITS.to_string())
        .append_pair("period", &STEP_SECS.to_string());
    format!("otpauth://totp/{label}?{}", query.finish())
}

/// What a URI path segment escapes: everything but the unreserved characters.
const URI_COMPONENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// The step `unix_secs` falls in.
pub(super) fn step(unix_secs: u64) -> u64 {
    unix_secs / STEP_SECS
}

/// The RFC 4226 code of `secret` for `counter`, `digits` long.
fn hotp(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC takes a key of any length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[digest.len() - 1] & 0x0f);
    let binary = u32::from_be_bytes([
        digest[offset] & 0x7f,
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]);
    binary % 10u32.pow(digits)
}

/// The step whose code `code` is, within [`SKEW`] of `now`, and later than `used`, the last step
/// a code of this secret signed in with: a code works once. `None` when no step matches.
pub(super) fn verify(secret: &[u8], code: &str, now: u64, used: Option<u64>) -> Option<u64> {
    let code = code.trim();
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut matched = None;
    // Every step is computed, so the time taken does not say which matched.
    for step in now.saturating_sub(SKEW)..=now + SKEW {
        let expected = format!(
            "{:0width$}",
            hotp(secret, step, DIGITS),
            width = DIGITS as usize
        );
        let fresh = used.is_none_or(|used| step > used);
        if bool::from(expected.as_bytes().ct_eq(code.as_bytes())) && fresh && matched.is_none() {
            matched = Some(step);
        }
    }
    matched
}

#[cfg(test)]
mod tests;
