//! Owns the failures Tablo raises itself; app and Topcoat errors pass through
//! untouched.

use std::fmt;

/// A failure Tablo raises itself.
#[derive(Debug)]
pub(crate) enum TabloError {
    /// Malformed `?after=`/`?before=` token; the list's retry link drops it.
    Cursor(String),
    /// Well-formed cursor a query's ordering refuses; same retry contract.
    CursorRejected(String),
    /// Panel or resource declaration that cannot work; names what to change.
    Declaration(String),
    /// Database or dependency failure; the message is the response's only text.
    Infrastructure(&'static str),
}

impl TabloError {
    /// The `TabloError` inside `error`, if Tablo raised it.
    pub(crate) fn of(error: &topcoat::Error) -> Option<&Self> {
        error.downcast_ref::<Self>()
    }

    /// Whether `error` is a cursor fault.
    pub(crate) fn is_cursor(error: &topcoat::Error) -> bool {
        matches!(
            Self::of(error),
            Some(Self::Cursor(_) | Self::CursorRejected(_))
        )
    }

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

/// Refuses to render a declaration with `errors`.
pub(crate) fn misdeclared(errors: &[crate::DeclarationErrorKind]) -> topcoat::Error {
    declaration(
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// Raises a declaration failure, logging the message for operators.
pub(crate) fn declaration(message: impl Into<String>) -> topcoat::Error {
    let message = message.into();
    tracing::error!(error = %message, "declaration failure");
    TabloError::Declaration(message).into()
}

pub(crate) const DATABASE_UNAVAILABLE: &str = "database unavailable";

/// Maps a database failure to an opaque 500, logging the cause for operators.
pub(crate) fn unavailable(source: impl fmt::Display) -> topcoat::Error {
    tracing::error!(error = %source, "database unavailable");
    TabloError::Infrastructure(DATABASE_UNAVAILABLE).into()
}

/// Maps a [`toasty::Error`] to an infrastructure failure and passes anything else
/// through with its mapping intact.
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
