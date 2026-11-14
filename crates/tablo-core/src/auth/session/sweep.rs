//! The sweep a successful login runs: expired session rows, a bounded batch at a time.

use jiff::Timestamp;
use topcoat::context::Cx;

use super::AuthSession;
use crate::auth::infrastructure_failure;

/// How many expired session rows one sweep drops: the bound that keeps one
/// login from deleting an unbounded number of rows.
pub(in crate::auth) const SESSION_SWEEP_BATCH: usize = 500;

/// Drops up to [`SESSION_SWEEP_BATCH`] expired session rows on successful login.
pub(in crate::auth) async fn sweep_expired_sessions(cx: &Cx) -> topcoat::Result<()> {
    let now = Timestamp::now();
    let mut db = crate::db::db(cx)?;
    let expired: Vec<String> = AuthSession::filter(AuthSession::fields().expires_at().le(now))
        .limit(SESSION_SWEEP_BATCH)
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?
        .into_iter()
        .map(|row| row.token_hash)
        .collect();
    if expired.is_empty() {
        return Ok(());
    }
    // The expiry is re-checked: the select and the delete are two statements.
    AuthSession::filter(
        AuthSession::fields()
            .token_hash()
            .in_list(expired)
            .and(AuthSession::fields().expires_at().le(now)),
    )
    .delete()
    .exec(&mut db)
    .await
    .map_err(infrastructure_failure)?;
    Ok(())
}
