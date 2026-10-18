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

/// How long a session stays valid: seven days, fixed.
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
        None,
        expires_at,
    )
    .await
    .map_err(infrastructure_failure)
}

/// The panels whose authenticator loads `user_type`, or `own` outside a router: the panels that
/// share a user's credentials, so a credential change ends the user's sessions and reset links on
/// each.
pub(super) fn credential_panels(
    cx: &Cx,
    user_type: std::any::TypeId,
    own: &Arc<PanelState>,
) -> Vec<Arc<PanelState>> {
    crate::panel::state::panels(cx).map_or_else(
        || vec![Arc::clone(own)],
        |panels| {
            panels
                .0
                .iter()
                .filter(|panel| {
                    panel
                        .auth
                        .authenticator()
                        .is_some_and(|auth| auth.user_type() == user_type)
                })
                .cloned()
                .collect()
        },
    )
}

/// Deletes every session `user_id` holds on `panels`, and every reset link it holds on those that
/// offer a reset, through `ex`: what a credential change ends.
pub(super) async fn end_credentials(
    ex: &mut dyn toasty::Executor,
    user_id: &str,
    panels: &[Arc<PanelState>],
) -> topcoat::Result<()> {
    let prefixes: Vec<String> = panels.iter().map(|panel| panel.prefix.clone()).collect();
    AuthSession::filter(
        AuthSession::fields()
            .user_id()
            .eq(user_id.to_string())
            .and(AuthSession::fields().panel().in_list(prefixes)),
    )
    .delete()
    .exec(&mut *ex)
    .await
    .map_err(infrastructure_failure)?;
    let resets: Vec<String> = panels
        .iter()
        .filter(|panel| panel.auth.reset().is_some())
        .map(|panel| panel.prefix.clone())
        .collect();
    if resets.is_empty() {
        // Without a reset the token model may be unregistered.
        return Ok(());
    }
    super::password_reset::end_links(ex, user_id, Some(resets)).await
}

/// Replaces every session `user_id` holds, on every panel whose authenticator loads `user_type`,
/// with `session` on the panel and tenant `signed` acts under, and ends its reset links there,
/// through `ex`: what a credential change does inside the transaction that writes it, so the new
/// credential and the revocation commit together.
pub(super) async fn replace_sessions(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    signed: &SignedIn,
    user_type: std::any::TypeId,
    session: &session::Session,
) -> topcoat::Result<()> {
    let panels = credential_panels(cx, user_type, &signed.panel);
    let user_id = signed.user.user_id();
    end_credentials(&mut *ex, &user_id, &panels).await?;
    let expires_at = Timestamp::try_from(session.expires_at).map_err(topcoat::Error::from)?;
    insert(
        ex,
        &session.token_hash,
        user_id,
        signed.panel.prefix.clone(),
        signed.tenant,
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
        None,
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
    tenant: Option<Uuid>,
    expires_at: Timestamp,
) -> toasty::Result<()> {
    toasty::create!(AuthSession {
        token_hash: token_key(hash),
        user_id,
        panel,
        tenant,
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

/// Ends the session the request presents and starts a new one, so a token presented before a
/// sign-in cannot be replayed after it.
///
/// # Errors
///
/// The opaque sign-in outage when the database cannot answer; any other failure keeps its mapping.
pub(super) async fn fresh_session(cx: &Cx) -> topcoat::Result<session::Session> {
    if let Some(hash) = session::token_hash(cx)
        .await
        .map_err(infrastructure_failure)?
    {
        delete_session(cx, &hash).await?;
    }
    session::start(cx).await.map_err(infrastructure_failure)
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

/// Revokes every live session and reset link of `user_id` on the current panel and on each panel
/// sharing its users, whose authenticator loads the same user type, or on every panel outside any
/// panel; call it whenever a credential changes.
///
/// # Errors
///
/// The opaque sign-in outage when the database cannot answer.
pub async fn revoke_sessions_for_user(cx: &Cx, user_id: &str) -> topcoat::Result<()> {
    let mut db = crate::db::db(cx);
    if let Some(panel) = current(cx) {
        let panels = match panel.auth.authenticator() {
            Some(auth) => credential_panels(cx, auth.user_type(), panel),
            None => vec![Arc::clone(panel)],
        };
        return end_credentials(&mut db, user_id, &panels).await;
    }
    AuthSession::filter(AuthSession::fields().user_id().eq(user_id.to_string()))
        .delete()
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?;
    let resets = crate::panel::state::panels(cx)
        .is_some_and(|panels| panels.0.iter().any(|panel| panel.auth.reset().is_some()));
    if resets {
        super::password_reset::end_links(&mut db, user_id, None).await?;
    }
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
