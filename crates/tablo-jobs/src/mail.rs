//! Auth mail made durable: [`AuthMailQueue`] queues each letter a panel's auth sends, sealed,
//! inside the transaction that records the letter's token, and a worker sends it.

use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use data_encoding::BASE64URL_NOPAD;
use hkdf::Hkdf;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tablo_core::{AuthLetter, AuthMailDelivery};
use topcoat::{
    context::{Cx, context},
    cookie::Key,
};

use crate::{Job, enqueue};

/// What the derivation binds the seal's key to: no other use of the cookie key derives this one.
const PURPOSE: &[u8] = b"tablo-jobs auth mail v1";

/// The bytes of a GCM nonce, at the head of a sealed letter.
const NONCE_BYTES: usize = 12;

/// Queues the letters a panel's auth sends, for a [`Worker`](crate::Worker) with
/// [`auth_mail`](crate::Worker::auth_mail) to send.
///
/// ```rust,no_run
/// # use tablo_core::{Auth, AuthMail, PanelHandle};
/// # use tablo_jobs::{AuthMailQueue, Worker};
/// # use topcoat::{cookie::Key, mail::Mailbox};
/// # fn build(key: &Key, panel: &PanelHandle, from: Mailbox) -> topcoat::Result<()> {
/// let queue = AuthMailQueue::new(key);
/// let mail = AuthMail::new(from, "https://app.example.com").delivery(queue.clone());
/// let worker = Worker::new(panel)?.auth_mail(&queue);
/// # let _ = (mail, worker);
/// # Ok(())
/// # }
/// ```
///
/// - A letter commits with its token or not at all, and the worker retries it like any job: an
///   outage of the mail server delays it instead of losing it.
/// - A letter carries a live link or code, so the queue stores it sealed: AES-256-GCM under a key
///   derived from the router's cookie [`Key`]. A reader of the database or its backups without that
///   key reads no link.
/// - The web process and the worker build the queue from the same key. A key rotated while mail
///   waits leaves that mail unopenable: it dies after its attempts, and the person asks again.
/// - A letter past its [`expires_at`](AuthLetter::expires_at) is dropped, not sent: a link that
///   arrives dead reads as broken. Six attempts span about a quarter of an hour of backoff.
#[derive(Clone)]
pub struct AuthMailQueue {
    cipher: Aes256Gcm,
}

impl AuthMailQueue {
    /// A queue sealing letters under a key derived from `key`, the router's cookie key.
    #[must_use]
    pub fn new(key: &Key) -> Self {
        let mut derived = [0; 32];
        Hkdf::<Sha256>::new(None, key.master())
            .expand(PURPOSE, &mut derived)
            .expect("32 bytes is a valid HKDF-SHA256 output length");
        Self {
            cipher: Aes256Gcm::new(&derived.into()),
        }
    }

    fn seal(&self, plain: &[u8]) -> topcoat::Result<String> {
        let mut nonce = [0; NONCE_BYTES];
        getrandom::fill(&mut nonce)
            .map_err(|error| topcoat::Error::msg(format!("no randomness for a nonce: {error}")))?;
        let sealed = self
            .cipher
            .encrypt(
                &Nonce::from(nonce),
                Payload {
                    msg: plain,
                    aad: SendAuthMail::KIND.as_bytes(),
                },
            )
            .map_err(|_| topcoat::Error::msg("sealing an auth letter failed"))?;
        let mut stored = nonce.to_vec();
        stored.extend(sealed);
        Ok(BASE64URL_NOPAD.encode(&stored))
    }

    fn open(&self, stored: &str) -> topcoat::Result<Vec<u8>> {
        let refused = || {
            topcoat::Error::msg("a queued auth letter does not open: was the cookie key changed?")
        };
        let stored = BASE64URL_NOPAD
            .decode(stored.as_bytes())
            .map_err(|_| refused())?;
        if stored.len() < NONCE_BYTES {
            return Err(refused());
        }
        let (nonce, sealed) = stored.split_at(NONCE_BYTES);
        self.cipher
            .decrypt(
                &Nonce::try_from(nonce).map_err(|_| refused())?,
                Payload {
                    msg: sealed,
                    aad: SendAuthMail::KIND.as_bytes(),
                },
            )
            .map_err(|_| refused())
    }
}

impl AuthMailDelivery for AuthMailQueue {
    async fn deliver(
        &self,
        _cx: &Cx,
        ex: &mut dyn toasty::Executor,
        letter: AuthLetter,
    ) -> topcoat::Result<()> {
        let sealed = self.seal(&serde_json::to_vec(&letter)?)?;
        enqueue(&SendAuthMail { sealed }).exec(ex).await
    }
}

/// Sends a letter an [`AuthMailQueue`] queued.
#[derive(Serialize, Deserialize)]
pub(crate) struct SendAuthMail {
    /// An [`AuthLetter`] as JSON, sealed.
    sealed: String,
}

impl Job for SendAuthMail {
    const KIND: &'static str = "tablo-auth-mail";
    const MAX_ATTEMPTS: u32 = 6;

    async fn run(self, cx: &Cx) -> topcoat::Result<()> {
        let queue: &AuthMailQueue = context(cx);
        let letter: AuthLetter = serde_json::from_slice(&queue.open(&self.sealed)?)?;
        if letter.expires_at().is_some_and(|at| at <= Timestamp::now()) {
            tracing::warn!(kind = ?letter.kind(), "an auth letter expired in the queue: dropped");
            return Ok(());
        }
        topcoat::mail::send(cx, letter.mail()?).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
