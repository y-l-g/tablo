//! The counts of a [`LoginThrottle::in_database`](super::LoginThrottle::in_database): one
//! [`AuthThrottle`] row per login and address, which every process reads.

use std::{fmt::Write as _, net::IpAddr};

use jiff::{SignedDuration, Timestamp};
use sha2::{Digest, Sha256};
use topcoat::context::Cx;

use super::Limit;
use crate::auth::{infrastructure_failure, realm::panel_prefix, token::run_serializable};

/// Shipped sign-in throttle record: how many attempts one login made from one address since the
/// window began, for a [`LoginThrottle`](super::LoginThrottle) that counts in the database.
#[derive(Debug, Clone, toasty::Model)]
pub struct AuthThrottle {
    /// Hex-encoded SHA-256 of the throttle, the login and the address, so the table holds no
    /// login in the clear.
    #[key]
    pub key: String,
    /// The panel's prefix and what the throttle counts, such as `/admin:sign-in`.
    #[index]
    pub scope: String,
    pub attempts: i64,
    /// When the window began: the first attempt it counts.
    pub since: Timestamp,
}

/// Where a count lives: its throttle's scope and its own key.
pub(super) struct Slot {
    scope: String,
    key: String,
}

impl Slot {
    /// The count of `login` from `client` for the throttle of `scope` on the request's panel.
    pub(super) fn of(cx: &Cx, scope: &str, login: &str, client: Option<IpAddr>) -> Self {
        let scope = format!("{}:{scope}", panel_prefix(cx));
        let client = client.map(|client| client.to_string()).unwrap_or_default();
        let mut hash = Sha256::new();
        for part in [
            scope.as_str(),
            login.trim().to_lowercase().as_str(),
            &client,
        ] {
            // The length first, so no two splits of the same bytes hash alike.
            hash.update(part.len().to_be_bytes());
            hash.update(part.as_bytes());
        }
        let mut key = String::with_capacity(64);
        for byte in hash.finalize() {
            write!(key, "{byte:02x}").expect("writing to a String cannot fail");
        }
        Self { scope, key }
    }
}

/// Counts an attempt in `slot`, or answers `false` when it is past `limit`.
pub(super) async fn attempt(cx: &Cx, limit: Limit, slot: &Slot) -> topcoat::Result<bool> {
    let now = Timestamp::now();
    let began = now
        .checked_sub(SignedDuration::try_from(limit.window).map_err(topcoat::Error::from)?)
        .map_err(topcoat::Error::from)?;
    let fresh = run_serializable!(cx, |tx| async {
        let row = AuthThrottle::filter(AuthThrottle::fields().key().eq(slot.key.clone()))
            .first()
            .exec(&mut *tx)
            .await
            .map_err(infrastructure_failure)?;
        match row {
            Some(row) if row.since > began => {
                if row.attempts >= i64::from(limit.attempts) {
                    return Ok(None);
                }
                AuthThrottle::filter(AuthThrottle::fields().key().eq(slot.key.clone()))
                    .update()
                    .attempts(row.attempts + 1)
                    .exec(&mut *tx)
                    .await
                    .map_err(infrastructure_failure)?;
                Ok(Some(false))
            }
            stale => {
                if stale.is_some() {
                    AuthThrottle::filter(AuthThrottle::fields().key().eq(slot.key.clone()))
                        .delete()
                        .exec(&mut *tx)
                        .await
                        .map_err(infrastructure_failure)?;
                }
                toasty::create!(AuthThrottle {
                    key: slot.key.clone(),
                    scope: slot.scope.clone(),
                    attempts: 1,
                    since: now,
                })
                .exec(&mut *tx)
                .await
                .map_err(infrastructure_failure)?;
                Ok(Some(true))
            }
        }
    })?;
    let Some(fresh) = fresh else {
        return Ok(false);
    };
    if fresh {
        // Outside the transaction: a sweep inside it would make concurrent attempts conflict.
        prune(cx, &slot.scope, began).await?;
    }
    Ok(true)
}

/// Deletes the count in `slot`, after its login signs in.
pub(super) async fn clear(cx: &Cx, slot: &Slot) -> topcoat::Result<()> {
    let mut db = crate::db::db(cx)?;
    AuthThrottle::filter(AuthThrottle::fields().key().eq(slot.key.clone()))
        .delete()
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)
}

/// Deletes the counts of `scope` whose window began before `began`, so logins that never come
/// back leave no rows.
async fn prune(cx: &Cx, scope: &str, began: Timestamp) -> topcoat::Result<()> {
    let mut db = crate::db::db(cx)?;
    AuthThrottle::filter(
        AuthThrottle::fields()
            .scope()
            .eq(scope.to_owned())
            .and(AuthThrottle::fields().since().le(began)),
    )
    .delete()
    .exec(&mut db)
    .await
    .map_err(infrastructure_failure)
}
