//! Seals an authenticator secret before it is stored, and hashes a recovery code or a mailed
//! code, under keys derived from the router's cookie [`Key`]: a database read alone yields none.

use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use data_encoding::BASE64URL_NOPAD;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use topcoat::cookie::Key;

/// What the derivation binds the key to: no other use of the cookie key derives this one.
const PURPOSE: &[u8] = b"tablo two-factor secret v1";

/// What the recovery-code derivation binds its key to.
const RECOVERY_PURPOSE: &[u8] = b"tablo two-factor recovery code v1";

/// What the code-hash derivation binds its key to.
const CODE_PURPOSE: &[u8] = b"tablo two-factor mailed code v1";

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

/// `value` hashed for the row `binding` names: HMAC-SHA256 under the key `purpose` derives from
/// `key`, in hex.
fn keyed_hash(key: &Key, purpose: &[u8], binding: &str, value: &str) -> String {
    let mut derived = [0; 32];
    Hkdf::<Sha256>::new(None, key.master())
        .expand(purpose, &mut derived)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&derived)
        .expect("HMAC takes a key of any length");
    mac.update(binding.as_bytes());
    mac.update(b"\n");
    mac.update(value.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A recovery code as stored, for the row `binding` names. Its fifty bits would fall to an
/// offline search of a plain hash, and one search would serve every row of an unsalted one: the
/// key and the binding keep both out of reach.
pub(super) fn recovery_hash(key: &Key, binding: &str, code: &str) -> String {
    keyed_hash(key, RECOVERY_PURPOSE, binding, code)
}

/// A mailed code as stored, for the row `binding` names. Six digits hash to a million values,
/// which a database read alone could try in an instant: the key keeps them out of reach.
pub(super) fn code_hash(key: &Key, binding: &str, code: &str) -> String {
    keyed_hash(key, CODE_PURPOSE, binding, code.trim())
}

/// `secret` sealed under `key` for the row `aad` names: a random nonce and the ciphertext, in
/// URL-safe base64. A sealed secret copied to another row does not open there.
pub(super) fn seal(key: &Key, secret: &[u8], aad: &str) -> String {
    let mut nonce = [0; NONCE_BYTES];
    getrandom::fill(&mut nonce).expect("the two-factor nonce randomness source failed");
    let sealed = cipher(key)
        .encrypt(
            &Nonce::from(nonce),
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
            &Nonce::try_from(nonce).ok()?,
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

    /// A recovery code hashes apart for each key and row.
    #[test]
    fn a_recovery_hash_is_bound_to_its_key_and_row() {
        let key = Key::generate();
        let hash = recovery_hash(&key, "user:/admin", "abcdefghij");
        assert_eq!(recovery_hash(&key, "user:/admin", "abcdefghij"), hash);
        assert_ne!(recovery_hash(&key, "other:/admin", "abcdefghij"), hash);
        assert_ne!(
            recovery_hash(&Key::generate(), "user:/admin", "abcdefghij"),
            hash
        );
    }

    /// A mailed code hashes apart for each key and row, and alike however it is padded.
    #[test]
    fn a_code_hash_is_bound_to_its_key_and_row() {
        let key = Key::generate();
        let hash = code_hash(&key, "user:/admin", "123456");
        assert_eq!(code_hash(&key, "user:/admin", " 123456 "), hash);
        assert_ne!(code_hash(&key, "other:/admin", "123456"), hash);
        assert_ne!(code_hash(&Key::generate(), "user:/admin", "123456"), hash);
    }
}
