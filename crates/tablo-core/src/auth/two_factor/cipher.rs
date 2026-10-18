//! Seals an authenticator secret before it is stored: AES-256-GCM under a key derived from the
//! router's cookie [`Key`], so a database read alone yields no secret.

use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use data_encoding::BASE64URL_NOPAD;
use hkdf::Hkdf;
use sha2::Sha256;
use topcoat::cookie::Key;

/// What the derivation binds the key to: no other use of the cookie key derives this one.
const PURPOSE: &[u8] = b"tablo two-factor secret v1";

/// The bytes of a GCM nonce.
const NONCE_BYTES: usize = 12;

/// The cipher [`seal`] and [`open`] use, derived from `key`.
fn cipher(key: &Key) -> Aes256Gcm {
    let mut derived = [0; 32];
    Hkdf::<Sha256>::new(None, key.master())
        .expand(PURPOSE, &mut derived)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    Aes256Gcm::new(&derived.into())
}

/// `secret` sealed under `key` for the row `aad` names: a random nonce and the ciphertext, in
/// URL-safe base64. A sealed secret copied to another row does not open there.
pub(super) fn seal(key: &Key, secret: &[u8], aad: &str) -> String {
    let mut nonce = [0; NONCE_BYTES];
    getrandom::fill(&mut nonce).expect("the two-factor nonce randomness source failed");
    let sealed = cipher(key)
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: secret,
                aad: aad.as_bytes(),
            },
        )
        .expect("AES-GCM seals any secret this small");
    let mut stored = nonce.to_vec();
    stored.extend(sealed);
    BASE64URL_NOPAD.encode(&stored)
}

/// The secret [`seal`] sealed for `aad`, or `None` when `stored` is not one: another key sealed
/// it, for another row, or it was tampered with.
pub(super) fn open(key: &Key, stored: &str, aad: &str) -> Option<Vec<u8>> {
    let stored = BASE64URL_NOPAD.decode(stored.as_bytes()).ok()?;
    if stored.len() < NONCE_BYTES {
        return None;
    }
    let (nonce, sealed) = stored.split_at(NONCE_BYTES);
    cipher(key)
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: sealed,
                aad: aad.as_bytes(),
            },
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sealed secret opens under its key for its row, and under nothing else.
    #[test]
    fn a_sealed_secret_opens_only_where_it_was_sealed() {
        let key = Key::generate();
        let sealed = seal(&key, b"secret", "user:/admin");
        assert_eq!(
            open(&key, &sealed, "user:/admin").as_deref(),
            Some(&b"secret"[..])
        );
        assert_eq!(open(&key, &sealed, "other:/admin"), None);
        assert_eq!(open(&Key::generate(), &sealed, "user:/admin"), None);
        assert_ne!(seal(&key, b"secret", "user:/admin"), sealed);
        assert_eq!(open(&key, "not base64!", "user:/admin"), None);
    }
}
