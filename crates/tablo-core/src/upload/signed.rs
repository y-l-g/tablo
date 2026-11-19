//! Signed, expiring URLs for the files a panel stores with
//! [`Panel::uploads_dir`](crate::Panel::uploads_dir): the directory answers a request only with a
//! signature a page of the panel rendered, so a file's bare URL opens nothing and a leaked link
//! stops working within two hours.
//!
//! Each signature covers a nonce drawn for it, so no two renders of a page hold the same URL. A
//! signature that stayed the same across renders would sit in every compressed page beside the
//! search the URL chooses, where an observer of the encrypted traffic could guess it from
//! response sizes (BREACH) and open the file without a session.

use std::time::{SystemTime, UNIX_EPOCH};

use data_encoding::BASE64URL_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use topcoat::context::Cx;

/// What the derivation binds the key to: no other use of the cookie key derives this one.
const PURPOSE: &[u8] = b"tablo upload url v1";

/// How long a signature lasts, in seconds.
pub(crate) const LIFETIME: u64 = 2 * 3600;

/// The random bytes in a nonce.
const NONCE_LEN: usize = 16;

const EXPIRES: &str = "expires";
const NONCE: &str = "nonce";
const SIGNATURE: &str = "signature";

/// The seconds since the Unix epoch.
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn mac(cx: &Cx, path: &str, expires: u64, nonce: &str) -> Hmac<Sha256> {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&super::key::derived(cx, PURPOSE))
        .expect("HMAC takes a key of any length");
    mac.update(path.as_bytes());
    mac.update(b"\n");
    mac.update(expires.to_string().as_bytes());
    mac.update(b"\n");
    mac.update(nonce.as_bytes());
    mac
}

/// `path` with a signature that lasts [`LIFETIME`] after `now`, under a fresh nonce.
pub(crate) fn sign(cx: &Cx, path: &str, now: u64) -> String {
    let expires = now + LIFETIME;
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce).expect("the upload URL nonce randomness source failed");
    let nonce = BASE64URL_NOPAD.encode(&nonce);
    let signature = BASE64URL_NOPAD.encode(&mac(cx, path, expires, &nonce).finalize().into_bytes());
    format!("{path}?{EXPIRES}={expires}&{NONCE}={nonce}&{SIGNATURE}={signature}")
}

/// Whether `query` holds a signature [`sign`] made for `path` that has not expired at `now`.
pub(crate) fn verify(cx: &Cx, path: &str, query: Option<&str>, now: u64) -> bool {
    let (mut expires, mut nonce, mut signature) = (None, None, None);
    for (key, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        match &*key {
            EXPIRES => expires = value.parse::<u64>().ok(),
            NONCE => nonce = Some(value.into_owned()),
            SIGNATURE => signature = BASE64URL_NOPAD.decode(value.as_bytes()).ok(),
            _ => {}
        }
    }
    let (Some(expires), Some(nonce), Some(signature)) = (expires, nonce, signature) else {
        return false;
    };
    expires > now
        && mac(cx, path, expires, &nonce)
            .verify_slice(&signature)
            .is_ok()
}

#[cfg(test)]
mod tests;
