//! A refused submission's pending upload, carried to the next one through `keep_<field>` under a
//! tag only the server mints: the path it names passed that field's rules for that browser.

use std::sync::OnceLock;

use data_encoding::BASE64URL_NOPAD;
use hkdf::Hkdf;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use topcoat::{
    context::{Cx, try_app_context},
    cookie::Key,
};

/// What the derivation binds the key to: no other use of the cookie key derives this one.
const PURPOSE: &[u8] = b"tablo carried upload v1";

/// The tag's key: derived from the router's cookie key, so any instance sharing it opens the
/// carry, else drawn once per process, so a carry opens only where it was minted.
fn key(cx: &Cx) -> [u8; 32] {
    static PROCESS: OnceLock<[u8; 32]> = OnceLock::new();
    let Some(cookie) = try_app_context::<Key>(cx) else {
        return *PROCESS.get_or_init(|| {
            let mut key = [0; 32];
            getrandom::fill(&mut key).expect("the carried-upload key randomness source failed");
            key
        });
    };
    let mut derived = [0; 32];
    Hkdf::<Sha256>::new(None, cookie.master())
        .expand(PURPOSE, &mut derived)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    derived
}

/// The MAC over `path` carried under `field` for the browser holding the CSRF cookie.
fn mac(cx: &Cx, field: &str, path: &str) -> Hmac<Sha256> {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&key(cx))
        .expect("HMAC takes a key of any length");
    for part in [field, &crate::csrf::current_token(cx), path] {
        mac.update(part.as_bytes());
        mac.update(b"\n");
    }
    mac
}

/// The `keep_<field>` value carrying `path` under `field`: its tag, a `.`, and the path.
pub(crate) fn seal(cx: &Cx, field: &str, path: &str) -> String {
    let tag = BASE64URL_NOPAD.encode(&mac(cx, field, path).finalize().into_bytes());
    format!("{tag}.{path}")
}

/// The path a `keep_<field>` value carries, when [`seal`] minted it for `field` and this browser.
pub(crate) fn open<'a>(cx: &Cx, field: &str, carried: &'a str) -> Option<&'a str> {
    let (tag, path) = carried.split_once('.')?;
    let tag = BASE64URL_NOPAD.decode(tag.as_bytes()).ok()?;
    mac(cx, field, path).verify_slice(&tag).ok()?;
    Some(path)
}
