//! The keys the upload module signs with, each bound to one purpose.

use std::sync::OnceLock;

use hkdf::Hkdf;
use sha2::Sha256;
use topcoat::{
    context::{Cx, try_app_context},
    cookie::Key,
};

/// The key for `purpose`: derived from the router's cookie key, so any instance sharing it
/// verifies what another signed, else from a secret drawn once per process, so a signature
/// verifies only where it was made. Drawing that secret logs a warning: behind a load balancer,
/// links break on every other replica.
pub(crate) fn derived(cx: &Cx, purpose: &[u8]) -> [u8; 32] {
    static PROCESS: OnceLock<[u8; 32]> = OnceLock::new();
    let master: &[u8] = match try_app_context::<Key>(cx) {
        Some(cookie) => cookie.master(),
        None => PROCESS.get_or_init(|| {
            tracing::warn!(
                "no cookie `Key` in the router's app context: upload links and carried uploads \
                 are signed with a key drawn for this process, so another replica or a restart \
                 refuses them"
            );
            let mut key = [0; 32];
            getrandom::fill(&mut key).expect("the upload key randomness source failed");
            key
        }),
    };
    let mut derived = [0; 32];
    Hkdf::<Sha256>::new(None, master)
        .expand(purpose, &mut derived)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    derived
}
