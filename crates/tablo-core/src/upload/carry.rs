//! A refused submission's pending upload, carried to the next one through `keep_<field>` under a
//! tag only the server mints: the path it names passed that field's rules for that browser.

use data_encoding::BASE64URL_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use topcoat::context::Cx;

/// What the derivation binds the key to: no other use of the cookie key derives this one.
const PURPOSE: &[u8] = b"tablo carried upload v1";

/// The MAC over `path` carried under `field` for the browser holding the CSRF cookie.
fn mac(cx: &Cx, field: &str, path: &str) -> Hmac<Sha256> {
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&super::key::derived(cx, PURPOSE))
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
