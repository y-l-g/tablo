//! [`LoginThrottle`]: counts sign-in attempts per login and client address, and refuses one past
//! its limit.

use std::{
    collections::HashMap,
    hash::{BuildHasher, RandomState},
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

/// The most counts a throttle keeps at once; past it, an expired count is dropped first, else the
/// oldest.
const MAX_TRACKED: usize = 10_000;

/// How many sign-in attempts one login may make from one client address in a window, as
/// [`Auth::throttle`](crate::Auth::throttle) sets it.
///
/// Each attempt that reaches the authenticator counts against the login it names, trimmed and
/// lowercased, and the address [`client_ip`](topcoat::router::request::client_ip) answers, from
/// the first attempt of the window; a successful sign-in clears that count. Past the limit, the
/// login page refuses the attempt without asking the authenticator, with the same 403 and message
/// as a wrong password, until the window ends, and logs a warning.
///
/// Keying on the address keeps a guesser from locking the account's owner out from elsewhere.
/// Behind a reverse proxy every request carries the proxy's address unless the router trusts it
/// through [`TrustedProxies`](topcoat::router::TrustedProxies); until then the count is per login
/// alone, and a guesser can lock the owner out.
///
/// It does not limit one client across logins, nor one login across many addresses, nor a login
/// spelled several ways that the authenticator's lookup treats as one, as MySQL's default
/// collation does with accents: a per-IP limit at the proxy covers those. Counts live in the
/// process and per panel, so each replica and each panel counts its own attempts, and a restart
/// clears them.
///
/// ```rust
/// use std::time::Duration;
///
/// use tablo_core::{Auth, LoginThrottle};
///
/// let auth = Auth::password().throttle(LoginThrottle::new(10, Duration::from_secs(300)));
/// # let _ = auth;
/// ```
pub struct LoginThrottle {
    limit: Option<Limit>,
    hasher: RandomState,
    counts: Mutex<HashMap<u64, Count>>,
}

#[derive(Debug, Clone, Copy)]
struct Limit {
    attempts: u32,
    window: Duration,
}

#[derive(Debug, Clone, Copy)]
struct Count {
    attempts: u32,
    since: Instant,
}

impl LoginThrottle {
    /// Allows `attempts` sign-in attempts per login in each `window`.
    ///
    /// # Panics
    ///
    /// When `attempts` is `0` or `window` is zero: neither allows a sign-in.
    #[must_use]
    pub fn new(attempts: u32, window: Duration) -> Self {
        assert!(attempts > 0, "a login throttle allows at least one attempt");
        assert!(!window.is_zero(), "a login throttle's window is not zero");
        Self::with(Some(Limit { attempts, window }))
    }

    /// Counts nothing, for a deployment whose proxy limits sign-ins.
    #[must_use]
    pub fn off() -> Self {
        Self::with(None)
    }

    fn with(limit: Option<Limit>) -> Self {
        Self {
            limit,
            hasher: RandomState::new(),
            counts: Mutex::new(HashMap::new()),
        }
    }

    /// Counts an attempt for `login` from `client`, or answers `false` when the pair is past its
    /// limit.
    pub(crate) fn attempt(&self, login: &str, client: Option<IpAddr>) -> bool {
        self.attempt_at(login, client, Instant::now())
    }

    fn attempt_at(&self, login: &str, client: Option<IpAddr>, now: Instant) -> bool {
        let Some(limit) = self.limit else {
            return true;
        };
        let key = self.key(login, client);
        let mut counts = self
            .counts
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if !counts.contains_key(&key) && counts.len() >= MAX_TRACKED {
            counts.retain(|_, count| now.duration_since(count.since) < limit.window);
            if counts.len() >= MAX_TRACKED
                && let Some(oldest) = counts
                    .iter()
                    .min_by_key(|(_, count)| count.since)
                    .map(|(key, _)| *key)
            {
                counts.remove(&oldest);
            }
        }
        let count = counts.entry(key).or_insert(Count {
            attempts: 0,
            since: now,
        });
        if now.duration_since(count.since) >= limit.window {
            *count = Count {
                attempts: 0,
                since: now,
            };
        }
        if count.attempts >= limit.attempts {
            return false;
        }
        count.attempts += 1;
        true
    }

    /// Clears the count of `login` from `client` after it signs in there.
    pub(crate) fn clear(&self, login: &str, client: Option<IpAddr>) {
        if self.limit.is_some() {
            let key = self.key(login, client);
            self.counts
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .remove(&key);
        }
    }

    /// The count's key: a hash of the login and the address, so the map holds no login in the
    /// clear.
    fn key(&self, login: &str, client: Option<IpAddr>) -> u64 {
        self.hasher.hash_one((login.trim().to_lowercase(), client))
    }
}

/// Five attempts per login and address a minute.
impl Default for LoginThrottle {
    fn default() -> Self {
        Self::new(5, Duration::from_secs(60))
    }
}

impl std::fmt::Debug for LoginThrottle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.limit {
            Some(Limit { attempts, window }) => f
                .debug_struct("LoginThrottle")
                .field("attempts", &attempts)
                .field("window", &window)
                .finish(),
            None => f.write_str("LoginThrottle::off"),
        }
    }
}

#[cfg(test)]
mod tests;
