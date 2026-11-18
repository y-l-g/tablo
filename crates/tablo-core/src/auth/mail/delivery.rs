//! Where auth mail goes once it is written: [`AuthLetter`], the mail itself, and
//! [`AuthMailDelivery`], the app's hand-off that takes it inside the transaction recording its
//! token.

use std::{future::Future, time::SystemTime};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use topcoat::{
    context::Cx,
    mail::{Mail, Mailbox},
};

use crate::BoxFuture;

/// One mail a panel's auth sends: a plain-text mail to one address, carrying a live link or code
/// for every [`kind`](Self::kind) but [`AuthLetterKind::TwoFactorAlert`].
///
/// It serializes, so an [`AuthMailDelivery`] can queue it, and [`mail`](Self::mail) rebuilds the
/// same mail each time, with the same `Date` and `Message-ID`: a letter sent twice after a crash
/// reads as one mail in a mail client. Its serialized form may change between releases, so a
/// queue drains before an upgrade, or drops what no longer deserializes.
///
/// A queued letter holds the link or code in clear: seal it before it reaches a database, a log
/// or a backup. Its `Debug` output leaves the subject and body out.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthLetter {
    kind: AuthLetterKind,
    from_name: Option<String>,
    from: String,
    to: String,
    subject: String,
    text: String,
    date: Timestamp,
    message_id: String,
    expires_at: Option<Timestamp>,
}

impl AuthLetter {
    /// What the letter is for.
    #[must_use]
    pub fn kind(&self) -> AuthLetterKind {
        self.kind
    }

    /// The address the letter goes to.
    #[must_use]
    pub fn to(&self) -> &str {
        &self.to
    }

    /// When the link or code the letter carries stops working, or `None` for a letter that
    /// carries none. A letter sent after it reads as broken: a queue drops it instead.
    #[must_use]
    pub fn expires_at(&self) -> Option<Timestamp> {
        self.expires_at
    }

    /// The letter as a mail to send through a [`Transport`](topcoat::mail::Transport).
    ///
    /// # Errors
    ///
    /// When an address no longer parses, which a letter from a later release may cause.
    pub fn mail(&self) -> topcoat::Result<Mail> {
        let from = match &self.from_name {
            Some(name) => Mailbox::named(name, &self.from)?,
            None => Mailbox::new(&self.from)?,
        };
        Ok(Mail::builder()
            .from(from)
            .to([Mailbox::new(&self.to)?])
            .subject(self.subject.clone())
            .text(self.text.clone())
            .date(SystemTime::from(self.date))
            .message_id(self.message_id.clone())
            .build()?)
    }

    /// A letter of `kind` from `from` to `to`, whose link or code expires at `expires_at`.
    pub(in crate::auth) fn new(
        kind: AuthLetterKind,
        from: &Mailbox,
        to: &str,
        subject: String,
        text: String,
        expires_at: Option<Timestamp>,
    ) -> topcoat::Result<Self> {
        // Built once to check the addresses and to fix the date and `Message-ID` it keeps.
        let mail = Mail::builder()
            .from(from.clone())
            .to([Mailbox::new(to)?])
            .subject(subject.clone())
            .text(text.clone())
            .build()?;
        Ok(Self {
            kind,
            from_name: from.name().map(str::to_string),
            from: from.address().to_string(),
            to: to.to_string(),
            subject,
            text,
            date: Timestamp::try_from(mail.date()).map_err(topcoat::Error::from)?,
            message_id: mail.message_id().to_string(),
            expires_at,
        })
    }
}

impl std::fmt::Debug for AuthLetter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthLetter")
            .field("kind", &self.kind)
            .field("to", &self.to)
            .field("message_id", &self.message_id)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// What an [`AuthLetter`] is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AuthLetterKind {
    /// A link that sets a new password, from
    /// [`Auth::password_reset`](super::super::Auth::password_reset).
    PasswordReset,
    /// A link that proves the account reads its address, from
    /// [`Auth::email_verification`](super::super::Auth::email_verification).
    EmailVerification,
    /// A link to a new address that makes it the account's.
    EmailChange,
    /// A link to the old address that cancels a pending email change.
    EmailChangeCancel,
    /// A six-digit code that signs in as a second factor, or sets the factor up.
    TwoFactorCode,
    /// A notice that the password signed in and second-factor codes failed after it. It carries
    /// no link or code.
    TwoFactorAlert,
}

/// Takes each [`AuthLetter`] a panel's auth sends, installed with
/// [`AuthMail::delivery`](super::AuthMail::delivery), so a durable queue sends it in place of the
/// router's [`Mailer`](topcoat::mail::Mailer).
///
/// Without one, a panel sends each letter through the `Mailer` once the token it carries
/// commits: a crash or a mail-server outage then loses it. A delivery writes the letter to a
/// queue through `ex`, the transaction that records its token, so the letter and the token
/// commit together or not at all, and a worker sends it later.
///
/// ```rust
/// use tablo_core::{AuthLetter, AuthMailDelivery};
/// use topcoat::context::Cx;
///
/// #[derive(Debug, toasty::Model)]
/// struct QueuedMail {
///     #[key]
///     #[auto]
///     id: u64,
///     #[column(type = text)]
///     sealed: String,
/// }
///
/// struct Outbox;
///
/// impl AuthMailDelivery for Outbox {
///     async fn deliver(
///         &self,
///         _cx: &Cx,
///         ex: &mut dyn toasty::Executor,
///         letter: AuthLetter,
///     ) -> topcoat::Result<()> {
///         let json = serde_json::to_string(&letter)?;
///         toasty::create!(QueuedMail {
///             sealed: seal(&json)
///         })
///         .exec(ex)
///         .await?;
///         Ok(())
///     }
/// }
/// # fn seal(json: &str) -> String { json.to_string() }
/// ```
///
/// - `deliver` runs inside the transaction recording the letter's token, through `ex`. An
///   [`AuthLetterKind::TwoFactorAlert`] records no token: `ex` is then the database itself.
/// - An error rolls the transaction back: no token is recorded and no letter goes out. A
///   password-reset or verification link is mailed after the response, so its failure is logged; a
///   two-factor code's answers the request with the outage; an email change's fails the save.
/// - It may run more than once for one letter when the database aborts the transaction for a
///   concurrent one, so it writes only through `ex`: a mail sent from inside it goes out once per
///   run, and before the token commits.
/// - The letter carries a live link or code. Seal it before writing it, and drop a letter past its
///   [`expires_at`](AuthLetter::expires_at) rather than send it late.
pub trait AuthMailDelivery: Send + Sync + 'static {
    /// Takes `letter` for sending, writing through `ex`.
    fn deliver(
        &self,
        cx: &Cx,
        ex: &mut dyn toasty::Executor,
        letter: AuthLetter,
    ) -> impl Future<Output = topcoat::Result<()>> + Send;
}

/// [`AuthMailDelivery`] made dyn-compatible.
pub(super) trait DynDelivery: Send + Sync {
    fn deliver<'a>(
        &'a self,
        cx: &'a Cx,
        ex: &'a mut dyn toasty::Executor,
        letter: AuthLetter,
    ) -> BoxFuture<'a, topcoat::Result<()>>;
}

impl<D: AuthMailDelivery> DynDelivery for D {
    fn deliver<'a>(
        &'a self,
        cx: &'a Cx,
        ex: &'a mut dyn toasty::Executor,
        letter: AuthLetter,
    ) -> BoxFuture<'a, topcoat::Result<()>> {
        Box::pin(AuthMailDelivery::deliver(self, cx, ex, letter))
    }
}

/// Sends `letter` through the router's [`Mailer`](topcoat::mail::Mailer).
pub(in crate::auth) async fn send(cx: &Cx, letter: AuthLetter) -> topcoat::Result<()> {
    topcoat::mail::send(cx, letter.mail()?).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
