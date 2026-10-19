//! Two-factor sign-in from a test's side: the secret and recovery codes the settings page shows,
//! and the code an authenticator app holding that secret would show.
//!
//! The code is computed here, apart from the panel's own check, so a test that signs in with it
//! pins RFC 6238 rather than agreeing with itself.

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use sha1::Sha1;

use crate::protocol::dom::{attr_value, strip_text, tags_where};

/// The authenticator key a set-up shows, in base32, or `None` when the page shows no set-up.
pub fn two_factor_secret(html: &str) -> Option<String> {
    let (_, _, inner) = tags_where(html, |_, attrs| {
        attrs
            .iter()
            .any(|(name, _)| name == "data-two-factor-secret")
    })
    .into_iter()
    .next()?;
    Some(strip_text(&inner))
}

/// The recovery codes the page shows, once, after a set-up or a regeneration; empty when it
/// shows none.
pub fn recovery_codes(html: &str) -> Vec<String> {
    let Some((_, _, inner)) = tags_where(html, |_, attrs| {
        attrs.iter().any(|(name, _)| name == "data-recovery-codes")
    })
    .into_iter()
    .next() else {
        return Vec::new();
    };
    tags_where(&inner, |tag, _| tag == "li")
        .into_iter()
        .map(|(_, _, code)| strip_text(&code))
        .collect()
}

/// Which view of the settings page renders: `off`, `setup` or `on`, as its form's
/// `data-two-factor` names it; `None` when the page shows recovery codes instead.
pub fn two_factor_state(html: &str) -> Option<String> {
    tags_where(html, |tag, attrs| {
        tag == "form" && attr_value(attrs, "data-two-factor").is_some()
    })
    .into_iter()
    .next()
    .and_then(|(_, attrs, _)| attr_value(&attrs, "data-two-factor"))
}

/// The six-digit code an authenticator app holding the base32 `secret` shows at `unix_secs`.
///
/// # Panics
///
/// When `secret` is not base32.
pub fn authenticator_code_at(secret: &str, unix_secs: u64) -> String {
    let key = BASE32_NOPAD
        .decode(secret.trim().as_bytes())
        .expect("the secret is base32");
    let mut mac = Hmac::<Sha1>::new_from_slice(&key).expect("HMAC takes a key of any length");
    mac.update(&(unix_secs / 30).to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[19] & 0x0f);
    let binary = u32::from_be_bytes([
        digest[offset] & 0x7f,
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]);
    format!("{:06}", binary % 1_000_000)
}

/// The code an authenticator app holding `secret` shows now.
pub fn authenticator_code(secret: &str) -> String {
    authenticator_code_at(secret, now())
}

/// Seconds since the Unix epoch.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The RFC 6238 SHA-1 vectors, truncated to six digits.
    #[test]
    fn the_code_matches_rfc_6238() {
        let secret = BASE32_NOPAD.encode(b"12345678901234567890");
        assert_eq!(authenticator_code_at(&secret, 59), "287082");
        assert_eq!(authenticator_code_at(&secret, 1_111_111_109), "081804");
        assert_eq!(authenticator_code_at(&secret, 2_000_000_000), "279037");
    }
}
