//! [`Sessions`]: how long a panel's sessions last, from sign-in and between requests.

use std::time::Duration;

use jiff::Timestamp;

use super::{AuthSession, SESSION_LIFETIME};
use crate::{Auth, PanelUser};

/// How long a panel's sessions last, set by [`Auth::sessions`]: seven days from sign-in by
/// default, however long the user stays away meanwhile.
///
/// ```rust
/// # use std::time::Duration;
/// # use tablo_core::{Auth, Sessions};
/// // A back office: sign in again each working day, and after half an hour away.
/// let auth = Auth::password().sessions(
///     Sessions::new()
///         .lifetime(Duration::from_secs(10 * 60 * 60))
///         .idle_timeout(Duration::from_secs(30 * 60)),
/// );
/// # let _ = auth;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sessions {
    lifetime: Duration,
    idle: Option<Duration>,
}

/// The shortest [`Sessions::lifetime`] and [`Sessions::idle_timeout`]: under it a user could not
/// finish signing in.
const SHORTEST: Duration = Duration::from_mins(1);

impl Sessions {
    /// Sessions that last [`SESSION_LIFETIME`] from sign-in, with no idle timeout.
    #[must_use]
    pub fn new() -> Self {
        Self {
            lifetime: SESSION_LIFETIME,
            idle: None,
        }
    }

    /// How long a session lasts from the sign-in that started it, whatever the user does
    /// meanwhile; never under a minute.
    ///
    /// It holds for the sessions already open: one older than a shortened lifetime ends on its
    /// next request. A credential change starts the lifetime again, with the session it issues.
    #[must_use]
    pub fn lifetime(mut self, lifetime: Duration) -> Self {
        self.lifetime = lifetime.max(SHORTEST);
        self
    }

    /// Ends a session once `idle` passes without a request from it, before its
    /// [`lifetime`](Self::lifetime) does; never under a minute. Off by default.
    ///
    /// Each request the panel's gate admits counts, a background one included; an open page that
    /// sends none, such as one showing the notification bell, does not. The session row records
    /// the end, moved forward at most once per tenth of `idle`, capped at five minutes, so a
    /// session may end that much early. A session opened before the timeout was set ends `idle`
    /// after its next request.
    #[must_use]
    pub fn idle_timeout(mut self, idle: Duration) -> Self {
        self.idle = Some(idle.max(SHORTEST));
        self
    }

    /// The lifetime: what a new session's cookie and row last.
    pub(in crate::auth) fn lasts(&self) -> Duration {
        self.lifetime
    }

    /// When a session starting at `now` first ends: the idle timeout, or the lifetime.
    pub(in crate::auth) fn first_end(&self, now: Timestamp) -> Timestamp {
        later(now, self.idle.unwrap_or(self.lifetime).min(self.lifetime))
    }

    /// What a request at `now` makes of `row`: whether it still stands, and the end to record.
    pub(in crate::auth) fn check(&self, row: &AuthSession, now: Timestamp) -> Standing {
        let last = later(row.created_at, self.lifetime);
        if row.expires_at <= now || last <= now {
            return Standing::Ended;
        }
        let Some(idle) = self.idle else {
            return Standing::Live(None);
        };
        let end = later(now, idle).min(last);
        let slack = (idle / 10).min(Duration::from_mins(5));
        let moved = end.duration_since(row.expires_at).unsigned_abs() > slack;
        Standing::Live(moved.then_some(end))
    }
}

impl Default for Sessions {
    fn default() -> Self {
        Self::new()
    }
}

/// What [`Sessions::check`] finds of a session row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::auth) enum Standing {
    /// Past its lifetime or its idle timeout.
    Ended,
    /// Standing, with the end a request that counts as activity records, when it moved.
    Live(Option<Timestamp>),
}

/// `from` plus `span`, or the latest timestamp when that overflows.
fn later(from: Timestamp, span: Duration) -> Timestamp {
    from.checked_add(span).unwrap_or(Timestamp::MAX)
}

impl<U: PanelUser> Auth<U> {
    /// How long the panel's sessions last: see [`Sessions`]. The default is seven days from
    /// sign-in, with no idle timeout.
    #[must_use]
    pub fn sessions(mut self, sessions: Sessions) -> Self {
        self.config.sessions = sessions;
        self
    }
}

#[cfg(test)]
mod tests;
