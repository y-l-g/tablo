//! The crate's own failures, and the one mapping of a driver failure.
//!
//! Every error Tablo raises itself is a [`TabloError`]; `?` converts it into
//! [`topcoat::Error`], and the seams that answer differently by kind read it
//! back with [`TabloError::of`]. An app's errors and Topcoat's own (a 404, a
//! 403) pass through untouched.

use std::fmt;

/// A failure Tablo raises itself.
#[derive(Debug)]
pub(crate) enum TabloError {
    /// A malformed `?after=`/`?before=` token. Retrying the identical URL can
    /// never succeed, so the list's retry link drops the cursor.
    Cursor(String),
    /// A well-formed cursor the query's ordering refuses: it was cut from a
    /// different `ORDER BY`. Same retry contract as [`Self::Cursor`].
    CursorRejected(String),
    /// A panel or resource declaration that cannot work. The message names
    /// what to change; it is the app author's, so it may name types.
    Declaration(String),
    /// The database, or another dependency, failed. The message is the only
    /// text the response carries; the cause went to the log.
    Infrastructure(&'static str),
}

impl TabloError {
    /// The `TabloError` inside `error`, if Tablo raised it.
    pub(crate) fn of(error: &topcoat::Error) -> Option<&Self> {
        error.downcast_ref::<Self>()
    }

    /// Whether `error` is the request's cursor's fault: a malformed token or
    /// a token the ordering rejects. The list page drops the cursor from its
    /// retry link, and the live retry resets it.
    pub(crate) fn is_cursor(error: &topcoat::Error) -> bool {
        matches!(
            Self::of(error),
            Some(Self::Cursor(_) | Self::CursorRejected(_))
        )
    }

    /// Whether `error` is an infrastructure failure.
    pub(crate) fn is_infrastructure(error: &topcoat::Error) -> bool {
        matches!(Self::of(error), Some(Self::Infrastructure(_)))
    }
}

impl fmt::Display for TabloError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Cursor(message) | Self::CursorRejected(message) | Self::Declaration(message) => {
                message.as_str()
            }
            Self::Infrastructure(message) => message,
        };
        f.write_str(message)
    }
}

impl std::error::Error for TabloError {}

/// The message a database failure carries.
pub(crate) const DATABASE_UNAVAILABLE: &str = "database unavailable";

/// Map a database infrastructure failure (pool or transaction open, probe,
/// statement, commit) to an opaque 500: `source` is logged for operators and
/// never reaches the page.
pub(crate) fn unavailable(source: impl fmt::Display) -> topcoat::Error {
    tracing::error!(error = %source, "database unavailable");
    TabloError::Infrastructure(DATABASE_UNAVAILABLE).into()
}

/// Map an error that may be the driver's: a [`toasty::Error`] is an
/// infrastructure failure, logged and answered with `message`; anything else is
/// the app's own (a record hook's guard, a custom authenticator's error) and
/// keeps its mapping, so a guard's 404 stays a 404.
///
/// The error's type settles which it is, never its text.
pub(crate) fn driver_failure(
    error: impl Into<topcoat::Error>,
    message: &'static str,
) -> topcoat::Error {
    let error = error.into();
    if error.is::<toasty::Error>() {
        tracing::error!(error = %error, answer = message, "infrastructure failure");
        TabloError::Infrastructure(message).into()
    } else {
        error
    }
}

#[cfg(test)]
mod tests;
