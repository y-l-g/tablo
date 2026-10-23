//! Signed, expiring URLs for the files a panel stores with
//! [`Panel::uploads_dir`](crate::Panel::uploads_dir): the directory answers a request only with a
//! signature a page of the panel rendered, so a file's bare URL opens nothing and a leaked link
//! stops working within two hours.

use std::time::{SystemTime, UNIX_EPOCH};

use data_encoding::BASE64URL_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use topcoat::context::Cx;

/// What the derivation binds the key to: no other use of the cookie key derives this one.
const PURPOSE: &[u8] = b"tablo upload url v1";

/// How long a signature lasts at least, in seconds; it lasts up to twice that. Rounding the
/// expiry to the window keeps a file's URL the same for an hour, so the browser caches it.
pub(crate) const WINDOW: u64 = 3600;

const EXPIRES: &str = "expires";
const SIGNATURE: &str = "signature";

/// The seconds since the Unix epoch.
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn mac(cx: &Cx, path: &str, expires: u64) -> Hmac<Sha256> {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&super::key::derived(cx, PURPOSE))
        .expect("HMAC takes a key of any length");
    mac.update(path.as_bytes());
    mac.update(b"\n");
    mac.update(expires.to_string().as_bytes());
    mac
}

/// `path` with a signature that lasts from one to two [`WINDOW`]s after `now`.
pub(crate) fn sign(cx: &Cx, path: &str, now: u64) -> String {
    let expires = (now / WINDOW + 2) * WINDOW;
    let signature = BASE64URL_NOPAD.encode(&mac(cx, path, expires).finalize().into_bytes());
    format!("{path}?{EXPIRES}={expires}&{SIGNATURE}={signature}")
}

/// Whether `query` holds a signature [`sign`] made for `path` that has not expired at `now`.
pub(crate) fn verify(cx: &Cx, path: &str, query: Option<&str>, now: u64) -> bool {
    let (mut expires, mut signature) = (None, None);
    for (key, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        match &*key {
            EXPIRES => expires = value.parse::<u64>().ok(),
            SIGNATURE => signature = BASE64URL_NOPAD.decode(value.as_bytes()).ok(),
            _ => {}
        }
    }
    let (Some(expires), Some(signature)) = (expires, signature) else {
        return false;
    };
    expires > now && mac(cx, path, expires).verify_slice(&signature).is_ok()
}

#[cfg(test)]
mod tests;
