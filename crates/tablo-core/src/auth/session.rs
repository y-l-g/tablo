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

/// The shipped server-side session record (ADR-0013): the SHA-256 hash of the
/// client token, the user it authenticates (opaque id), the panel that signed
/// the user in, the tenant the user selected, and its expiry.
///
/// The raw token is never stored; a leaked session table contains nothing a
/// client could present. Register this model alongside the app's user model.
#[derive(Debug, Clone, toasty::Model)]
pub struct AuthSession {
    /// Hex-encoded SHA-256 of the session token (the raw token stays client-side).
    #[key]
    pub token_hash: String,
    /// [`PanelUser::user_id`] of the authenticated user.
    #[index]
    pub user_id: String,
    /// The prefix of the panel that signed the user in (e.g. `/admin`): the
    /// only panel the session authenticates.
    pub panel: String,
    /// The tenant the user selected with the tenant switcher. It applies only
    /// while it is one of the user's [`tenants`](PanelUser::tenants); `None` acts
    /// for the first.
    pub tenant: Option<Uuid>,
    /// Indexed for the login sweep, whose filter is a range scan on this
    /// column; an app migrating an existing table adds the index with the
    /// model's own schema change.
    #[index]
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
}

/// Hex-encode a token hash into its storage key.
pub(super) fn token_key(hash: &TokenHash) -> String {
    use std::fmt::Write as _;

    let mut key = String::with_capacity(64);
    for byte in hash.iter() {
        write!(key, "{byte:02x}").expect("writing to a String cannot fail");
    }
    key
}

/// Record a new session for `user` on `panel`.
pub(super) async fn record(
    cx: &Cx,
    session: &session::Session,
    user: &dyn PanelUser,
    panel: &PanelState,
) -> topcoat::Result<()> {
    let mut db = crate::db::db(cx);
    toasty::create!(AuthSession {
        token_hash: token_key(&session.token_hash),
        user_id: user.user_id(),
        panel: panel.prefix.clone(),
        tenant: None,
        expires_at: Timestamp::try_from(session.expires_at).map_err(topcoat::Error::from)?,
        created_at: Timestamp::now(),
    })
    .exec(&mut db)
    .await
    .map_err(infrastructure_failure)?;
    Ok(())
}

/// Store `tenant` as the tenant the request's session acts for.
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

/// Delete the session row a token hash names, if any.
pub(super) async fn delete_session(cx: &Cx, hash: &TokenHash) -> topcoat::Result<()> {
    delete_session_row(cx, &token_key(hash)).await
}

/// Delete one stored session row by its hex token-hash key.
async fn delete_session_row(cx: &Cx, key: &str) -> topcoat::Result<()> {
    let mut db = crate::db::db(cx);
    AuthSession::filter(AuthSession::fields().token_hash().eq(key.to_string()))
        .delete()
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?;
    Ok(())
}

/// Revoke every live session of `user_id` (ADR-0013).
///
/// Call this whenever a credential changes out from under live sessions —
/// password reset/change and deactivation alike. Nothing in-core calls it
/// (there is no password-change flow in the framework); sessions otherwise
/// stay valid for their full fixed lifetime, so a reset that skips this
/// leaves a stolen session usable. A password-reset flow must call it,
/// and custom `Authenticator` apps own the same obligation.
///
/// On a panel's request it revokes the sessions that panel issued, since
/// another panel's `user_id` may name someone else; outside any panel it
/// revokes `user_id`'s sessions on every panel.
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

/// Drop up to [`SESSION_SWEEP_BATCH`] expired session rows, whoever owns them.
///
/// [`resolve`] purges a row when its token is looked up expired, so a row whose
/// token is never presented again stays in the table. A successful
/// login is where the sweep runs, before the new row: a visitor who never signs
/// in does not reach it. Toasty's `Delete` carries no `LIMIT`, so the bound
/// comes from selecting the keys first, and the select rides `expires_at`'s
/// index.
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
    // The expiry is re-read here: the select and the delete are two statements,
    // so a row whose lifetime was extended between them is no longer expired.
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

/// The request's live session row, lazily and without touching the database
/// when no session cookie is present. An expired row is purged on the way out.
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

/// The user `row` signs in to `panel` through `authenticator`. A row naming a
/// user who no longer authenticates (deleted or deactivated) is purged, so
/// removal is real (US11).
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

/// Resolve the request's session into `panel`'s user. A session another panel
/// issued resolves to no one here, and stays valid there.
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
