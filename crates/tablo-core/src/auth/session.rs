//! Server-side sessions: the [`AuthSession`] rows, their lookup, revocation
//! and sweep.

use std::{sync::Arc, time::Duration};

use jiff::Timestamp;
use topcoat::{
    context::Cx,
    session::{self, TokenHash},
};
use uuid::Uuid;

use super::{DynAuthenticator, PanelUser, SignedIn, infrastructure_failure};
use crate::panel::state::{PanelState, current};

/// How long a session stays valid: seven days, fixed (ADR-0013).
pub const SESSION_LIFETIME: Duration = Duration::from_hours(24 * 7);

/// Shipped server-side session record: the SHA-256 hash of the client token,
/// the user it authenticates, the panel that signed the user in, and its
/// expiry. The raw token is never stored.
#[derive(Debug, Clone, toasty::Model)]
pub struct AuthSession {
    /// Hex-encoded SHA-256 of the session token.
    #[key]
    pub token_hash: String,
    /// [`PanelUser::user_id`] of the authenticated user.
    #[index]
    pub user_id: String,
    /// The prefix of the panel that signed the user in (e.g. `/admin`): the
    /// only panel the session authenticates.
    pub panel: String,
    /// The tenant the user selected; `None` acts for the first.
    pub tenant: Option<Uuid>,
    /// Indexed for the login sweep's range scan on this column.
    #[index]
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
}

pub(super) fn token_key(hash: &TokenHash) -> String {
    use std::fmt::Write as _;

    let mut key = String::with_capacity(64);
    for byte in hash.iter() {
        write!(key, "{byte:02x}").expect("writing to a String cannot fail");
    }
    key
}

pub(super) async fn record(
    cx: &Cx,
    session: &session::Session,
    user: &dyn PanelUser,
    panel: &PanelState,
) -> topcoat::Result<()> {
    let expires_at = Timestamp::try_from(session.expires_at).map_err(topcoat::Error::from)?;
    insert(
        &mut crate::db::db(cx),
        &session.token_hash,
        user.user_id(),
        panel.prefix.clone(),
        expires_at,
    )
    .await
    .map_err(infrastructure_failure)
}

/// Records a session for `user` on the panel `Panel::new(panel)` mounts, as a successful login
/// does, and returns the token its session cookie carries. No credential is checked.
///
/// `tablo::testing`'s `TestClient::sign_in` is the caller: it lets a test act as a signed-in user
/// without a password hash.
#[doc(hidden)]
pub async fn mint_session(
    ex: &mut dyn toasty::Executor,
    panel: &str,
    user: &dyn PanelUser,
) -> topcoat::Result<String> {
    let token = session::Token::random();
    let expires_at = Timestamp::try_from(std::time::SystemTime::now() + SESSION_LIFETIME)
        .map_err(topcoat::Error::from)?;
    insert(
        ex,
        &token.hash(),
        user.user_id(),
        crate::panel::normalize_prefix(panel),
        expires_at,
    )
    .await?;
    Ok(token.encode())
}

async fn insert(
    ex: &mut dyn toasty::Executor,
    hash: &TokenHash,
    user_id: String,
    panel: String,
    expires_at: Timestamp,
) -> toasty::Result<()> {
    toasty::create!(AuthSession {
        token_hash: token_key(hash),
        user_id,
        panel,
        tenant: None,
        expires_at,
        created_at: Timestamp::now(),
    })
    .exec(ex)
    .await?;
    Ok(())
}

pub(super) async fn select_tenant(cx: &Cx, tenant: Uuid) -> topcoat::Result<()> {
    let Some(hash) = session::token_hash(cx).await? else {
        return Err(topcoat::router::error::forbidden().into());
    };
    let mut db = crate::db::db(cx);
    AuthSession::filter(AuthSession::fields().token_hash().eq(token_key(&hash)))
        .update()
        .tenant(Some(tenant))
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?;
    Ok(())
}

pub(super) async fn delete_session(cx: &Cx, hash: &TokenHash) -> topcoat::Result<()> {
    delete_session_row(cx, &token_key(hash)).await
}

async fn delete_session_row(cx: &Cx, key: &str) -> topcoat::Result<()> {
    let mut db = crate::db::db(cx);
    AuthSession::filter(AuthSession::fields().token_hash().eq(key.to_string()))
        .delete()
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?;
    Ok(())
}

/// Revokes every live session of `user_id` for the current panel, or on every
/// panel outside any panel; call it whenever a credential changes.
///
/// # Errors
///
/// The opaque sign-in outage when the database cannot answer.
pub async fn revoke_sessions_for_user(cx: &Cx, user_id: &str) -> topcoat::Result<()> {
    let mut db = crate::db::db(cx);
    let user = AuthSession::fields().user_id().eq(user_id.to_string());
    let sessions = match current(cx) {
        Some(panel) => {
            AuthSession::filter(user.and(AuthSession::fields().panel().eq(panel.prefix.clone())))
        }
        None => AuthSession::filter(user),
    };
    sessions
        .delete()
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?;
    Ok(())
}

/// How many expired session rows one sweep drops: the bound that keeps one
/// login from deleting an unbounded number of rows.
pub(super) const SESSION_SWEEP_BATCH: usize = 500;

/// Drops up to [`SESSION_SWEEP_BATCH`] expired session rows on successful login.
pub(super) async fn sweep_expired_sessions(cx: &Cx) -> topcoat::Result<()> {
    let now = Timestamp::now();
    let mut db = crate::db::db(cx);
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

/// Returns the request's live session row, purging it when expired and reading
/// nothing when no session cookie is present.
pub(super) async fn session_row(cx: &Cx) -> topcoat::Result<Option<AuthSession>> {
    let Some(hash) = session::token_hash(cx).await? else {
        return Ok(None);
    };
    let key = token_key(&hash);
    let mut db = crate::db::db(cx);
    let row = AuthSession::filter(AuthSession::fields().token_hash().eq(key))
        .first()
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.expires_at <= Timestamp::now() {
        delete_session_row(cx, &row.token_hash).await?;
        return Ok(None);
    }
    Ok(Some(row))
}

/// Resolves `row` to its user, purging the row when the user no longer
/// authenticates.
pub(super) async fn session_user(
    cx: &Cx,
    row: AuthSession,
    panel: &Arc<PanelState>,
    authenticator: &dyn DynAuthenticator,
) -> topcoat::Result<Option<SignedIn>> {
    match authenticator
        .find_by_id(cx, &row.user_id)
        .await
        .map_err(infrastructure_failure)?
    {
        Some(user) => Ok(Some(SignedIn {
            user,
            panel: Arc::clone(panel),
            tenant: row.tenant,
        })),
        None => {
            delete_session_row(cx, &row.token_hash).await?;
            Ok(None)
        }
    }
}

/// Resolves the request's session into `panel`'s user; a session another panel
/// issued resolves to no one here and stays valid there.
pub(super) async fn resolve(
    cx: &Cx,
    panel: &Arc<PanelState>,
    authenticator: &dyn DynAuthenticator,
) -> topcoat::Result<Option<SignedIn>> {
    match session_row(cx).await? {
        Some(row) if row.panel == panel.prefix => session_user(cx, row, panel, authenticator).await,
        _ => Ok(None),
    }
}
