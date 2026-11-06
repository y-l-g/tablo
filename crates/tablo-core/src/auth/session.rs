//! Server-side sessions: the [`AuthSession`] rows, their lookup, revocation
//! and sweep.

use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use jiff::Timestamp;
use topcoat::{
    context::Cx,
    session::{self, TokenHash, TokenStore, cookie::CookieTokenStore},
};

use super::{
    DynAuthenticator, PanelUser, SignedIn, infrastructure_failure,
    token::{CREDENTIALS, REVOCATION, TokenPurpose},
};
use crate::{
    TenantId,
    auth::realm::{Realm, current_realm, realms},
};

/// How long a session stays valid: seven days, fixed.
pub const SESSION_LIFETIME: Duration = Duration::from_hours(24 * 7);

/// What every panel's [`session_cookie`] starts with.
#[doc(hidden)]
pub const SESSION_COOKIE_PREFIX: &str = "__Host-tablo_session_";

/// The cookie that carries a session's token on the panel `Panel::new(panel)` mounts; `panel` is
/// spelled as for [`Panel::new`](crate::Panel::new). Each panel has its own, so a browser stays
/// signed in to several. Tablo keeps them apart from the router's
/// [`SessionConfig`](topcoat::session::SessionConfig), so a host app's own Topcoat sessions and a
/// panel's never replace each other's token.
#[must_use]
pub fn session_cookie(panel: &str) -> String {
    format!(
        "__Host-{}",
        store_name(&crate::naming::normalize_prefix(panel))
    )
}

/// The session cookie's name for `prefix`, as the store takes it: the store adds the `__Host-`
/// prefix, which fixes `Path=/`, so panels differ by name alone. A byte a cookie name may not hold
/// becomes `_` and its hex, and so does `_`, so no two prefixes share a name.
fn store_name(prefix: &str) -> String {
    use std::fmt::Write as _;

    let mut name = SESSION_COOKIE_PREFIX
        .trim_start_matches("__Host-")
        .to_owned();
    for byte in prefix.trim_start_matches('/').bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' {
            name.push(char::from(byte));
        } else {
            write!(name, "_{byte:02x}").expect("writing to a String cannot fail");
        }
    }
    name
}

/// The store of `panel`'s [`session_cookie`].
fn token_store(panel: &Realm) -> CookieTokenStore {
    CookieTokenStore::new().name(store_name(&panel.prefix))
}

/// The panel whose route is answering: only a panel's own routes start or end a session.
fn session_panel(cx: &Cx) -> topcoat::Result<&Arc<Realm>> {
    current_realm(cx).ok_or_else(|| {
        crate::error::declaration("a panel session changes only under a panel".to_owned())
    })
}

/// Shipped server-side session record: the SHA-256 hash of the client token,
/// the user it authenticates, the panel that signed the user in, and its
/// expiry. The raw token is never stored.
///
/// A session rides in its panel's [`session_cookie`], lasts [`SESSION_LIFETIME`] from login, is
/// replaced on each login and deleted on logout; each successful login also deletes up to 500
/// expired sessions. It belongs to the panel
/// that signed the user in: another panel's gate treats the request as anonymous, and
/// [`auth::user`](crate::auth::user) answers `None` there. A browser holds one session on each
/// panel, so signing in to a second panel leaves the first alone, and renaming a panel's prefix
/// ends the sessions it issued. Sessions stay valid until they expire unless
/// [`revoke_sessions_for_user`](crate::auth::revoke_sessions_for_user) ends them.
///
/// The row records the panel's prefix in a non-null `panel` column and the selected tenant in a
/// nullable `tenant` column, a [`TenantId`] Toasty stores as a `Uuid` on the database. A table
/// created without them gains them by migration: add `panel` with the panel's prefix as its
/// default (`ALTER TABLE auth_sessions ADD COLUMN panel TEXT NOT NULL DEFAULT '/admin'`), then drop
/// the default once every row names its panel.
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
    pub tenant: Option<TenantId>,
    /// Indexed for the login sweep's range scan on this column.
    #[index]
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
}

/// Starts a new session, issuing a fresh token in the request's panel's [`session_cookie`].
pub(super) async fn start(cx: &Cx) -> topcoat::Result<session::Session> {
    let token = session::Token::random();
    token_store(session_panel(cx)?)
        .write(cx, token.clone(), SESSION_LIFETIME)
        .await?;
    Ok(session::Session {
        token_hash: token.hash(),
        expires_at: SystemTime::now() + SESSION_LIFETIME,
    })
}

/// Discards the request's token, returning its hash so the caller can delete the row.
pub(super) async fn stop(cx: &Cx) -> topcoat::Result<Option<TokenHash>> {
    let panel = session_panel(cx)?;
    let hash = token_hash(cx, panel).await?;
    token_store(panel).delete(cx).await?;
    Ok(hash)
}

/// The hash of the token the request presents to `panel`; `None` when it presents none or a
/// malformed one.
async fn token_hash(cx: &Cx, panel: &Realm) -> topcoat::Result<Option<TokenHash>> {
    let token = token_store(panel).read(cx).await?;
    Ok(token.map(|token| token.hash()))
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
    panel: &Realm,
) -> topcoat::Result<()> {
    let expires_at = Timestamp::try_from(session.expires_at).map_err(topcoat::Error::from)?;
    insert(
        &mut crate::db::db(cx)?,
        &session.token_hash,
        user.user_id(),
        panel.prefix.clone(),
        None,
        expires_at,
    )
    .await
    .map_err(infrastructure_failure)
}

/// The realms whose authenticator loads `user_type`, or `own` outside a router: the panels that
/// share a user's credentials, so a credential change ends the user's sessions and reset links on
/// each.
pub(super) fn credential_panels(
    cx: &Cx,
    user_type: std::any::TypeId,
    own: &Arc<Realm>,
) -> Vec<Arc<Realm>> {
    let realms = realms(cx);
    if realms.is_empty() {
        return vec![Arc::clone(own)];
    }
    realms
        .into_iter()
        .filter(|realm| {
            realm
                .auth
                .authenticator()
                .is_some_and(|auth| auth.user_type() == user_type)
        })
        .collect()
}

/// Deletes every session `user_id` holds on `panels`, and every [`AuthToken`](super::AuthToken)
/// for one of `purposes` it holds on those that issue them, through `ex`: what a credential
/// change ends, with [`CREDENTIALS`], or a revocation, with [`REVOCATION`].
pub(super) async fn end_credentials(
    ex: &mut dyn toasty::Executor,
    user_id: &str,
    panels: &[Arc<Realm>],
    purposes: &[TokenPurpose],
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
    let issuing: Vec<String> = panels
        .iter()
        .filter(|panel| panel.auth.issues_tokens())
        .map(|panel| panel.prefix.clone())
        .collect();
    if issuing.is_empty() {
        // A panel that issues no token may leave its model unregistered.
        return Ok(());
    }
    super::token::end_all(ex, user_id, Some(issuing), purposes).await
}

/// Replaces every session `user_id` holds, on every panel whose authenticator loads `user_type`,
/// with `session` on the panel and tenant `signed` acts under, and ends its tokens there,
/// through `ex`: what a credential change does inside the transaction that writes it, so the new
/// credential and the revocation commit together.
pub(super) async fn replace_sessions(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    signed: &SignedIn,
    user_type: std::any::TypeId,
    session: &session::Session,
) -> topcoat::Result<()> {
    let panels = credential_panels(cx, user_type, &signed.realm);
    let user_id = signed.user.user_id();
    end_credentials(&mut *ex, &user_id, &panels, CREDENTIALS).await?;
    let expires_at = Timestamp::try_from(session.expires_at).map_err(topcoat::Error::from)?;
    insert(
        ex,
        &session.token_hash,
        user_id,
        signed.realm.prefix.clone(),
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
        crate::naming::normalize_prefix(panel),
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
    tenant: Option<TenantId>,
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

pub(super) async fn select_tenant(cx: &Cx, tenant: TenantId) -> topcoat::Result<()> {
    let Some(hash) = token_hash(cx, session_panel(cx)?).await? else {
        return Err(topcoat::router::error::forbidden().into());
    };
    let mut db = crate::db::db(cx)?;
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

/// The session `started` holds, or a new one it then holds: a transaction
/// [`run_serializable`](super::token::run_serializable) runs again records the session the first
/// run gave the browser, rather than one more cookie.
pub(super) async fn start_once(
    cx: &Cx,
    started: &mut Option<session::Session>,
) -> topcoat::Result<session::Session> {
    if let Some(session) = started {
        return Ok(session.clone());
    }
    let session = start(cx).await?;
    *started = Some(session.clone());
    Ok(session)
}

/// Ends the session the request presents and starts a new one, so a token presented before a
/// sign-in cannot be replayed after it.
///
/// # Errors
///
/// The opaque sign-in outage when the database cannot answer; any other failure keeps its mapping.
pub(super) async fn fresh_session(cx: &Cx) -> topcoat::Result<session::Session> {
    let panel = session_panel(cx)?;
    if let Some(hash) = token_hash(cx, panel)
        .await
        .map_err(infrastructure_failure)?
    {
        delete_session(cx, &hash).await?;
    }
    start(cx).await.map_err(infrastructure_failure)
}

async fn delete_session_row(cx: &Cx, key: &str) -> topcoat::Result<()> {
    let mut db = crate::db::db(cx)?;
    AuthSession::filter(AuthSession::fields().token_hash().eq(key.to_string()))
        .delete()
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?;
    Ok(())
}

/// Revokes every live session of `user_id`, its pending sign-ins, its reset links and its pending
/// email change, on the current panel and on each panel sharing its users, whose authenticator
/// loads the same user type, or on every panel outside any panel; call it whenever a credential
/// changes. A link that proves an address or undoes an email change survives.
///
/// # Errors
///
/// The opaque sign-in outage when the database cannot answer.
pub async fn revoke_sessions_for_user(cx: &Cx, user_id: &str) -> topcoat::Result<()> {
    let mut db = crate::db::db(cx)?;
    if let Some(panel) = current_realm(cx) {
        let panels = match panel.auth.authenticator() {
            Some(auth) => credential_panels(cx, auth.user_type(), panel),
            None => vec![Arc::clone(panel)],
        };
        end_credentials(&mut db, user_id, &panels, REVOCATION).await?;
        wake_bells(
            cx,
            user_id,
            panels.iter().map(|panel| panel.prefix.as_str()),
        );
        return Ok(());
    }
    AuthSession::filter(AuthSession::fields().user_id().eq(user_id.to_string()))
        .delete()
        .exec(&mut db)
        .await
        .map_err(infrastructure_failure)?;
    let realms = realms(cx);
    if realms.iter().any(|realm| realm.auth.issues_tokens()) {
        super::token::end_all(&mut db, user_id, None, REVOCATION).await?;
    }
    wake_bells(
        cx,
        user_id,
        realms.iter().map(|realm| realm.prefix.as_str()),
    );
    Ok(())
}

/// Wakes the bells `user_id` has open on `panels`, so each checks its session again and stops.
fn wake_bells<'a>(cx: &Cx, user_id: &str, panels: impl Iterator<Item = &'a str>) {
    let bus = crate::bus::bus(cx);
    for panel in panels {
        bus.wake(panel, user_id);
    }
}

/// How many expired session rows one sweep drops: the bound that keeps one
/// login from deleting an unbounded number of rows.
pub(super) const SESSION_SWEEP_BATCH: usize = 500;

/// Drops up to [`SESSION_SWEEP_BATCH`] expired session rows on successful login.
pub(super) async fn sweep_expired_sessions(cx: &Cx) -> topcoat::Result<()> {
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

/// Returns the live session row the request presents to `panel`, purging it when expired and
/// reading nothing when `panel`'s cookie is absent. A token moved over from another panel's cookie
/// finds no row here, since a row serves only the panel that issued it.
pub(super) async fn session_row(cx: &Cx, panel: &Realm) -> topcoat::Result<Option<AuthSession>> {
    let Some(hash) = token_hash(cx, panel).await? else {
        return Ok(None);
    };
    let key = token_key(&hash);
    let mut db = crate::db::db(cx)?;
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
    Ok((row.panel == panel.prefix).then_some(row))
}

/// Resolves `row` to its user, purging the row when the user no longer authenticates.
///
/// A panel behind [`ExternalAuth`](crate::ExternalAuth) keeps it: there `None` speaks for this
/// request's copy of the app's cookies, which a live connection opened before a rotation still
/// holds, and the browser's fresh tab goes on using the row. It serves no one the app does not
/// name, and the next entry for another user replaces it.
pub(super) async fn session_user(
    cx: &Cx,
    row: AuthSession,
    panel: &Arc<Realm>,
    authenticator: &dyn DynAuthenticator,
) -> topcoat::Result<Option<SignedIn>> {
    match authenticator
        .find_by_id(cx, &row.user_id)
        .await
        .map_err(infrastructure_failure)?
    {
        Some(user) => Ok(Some(SignedIn {
            user,
            realm: Arc::clone(panel),
            tenant: row.tenant,
        })),
        None => {
            if panel.auth.external().is_none() {
                delete_session_row(cx, &row.token_hash).await?;
            }
            Ok(None)
        }
    }
}

/// Resolves the request's session on `panel` into its user.
pub(super) async fn resolve(
    cx: &Cx,
    panel: &Arc<Realm>,
    authenticator: &dyn DynAuthenticator,
) -> topcoat::Result<Option<SignedIn>> {
    match session_row(cx, panel).await? {
        Some(row) => session_user(cx, row, panel, authenticator).await,
        None => Ok(None),
    }
}
