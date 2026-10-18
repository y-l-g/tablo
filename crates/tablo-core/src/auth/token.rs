//! The single-use secrets auth hands out besides sessions: the [`AuthToken`] rows a mailed link
//! or a pending sign-in carries, issued, read, used and ended here.

use std::time::Duration;

use jiff::Timestamp;
use topcoat::{context::Cx, session::Token};

use super::{infrastructure_failure, session::token_key};
use crate::{
    panel::{panel_prefix, state::current},
    toasty_compat::IsolationLevel,
};

/// Shipped single-use secret record: the SHA-256 hash of the token a mailed link or a pending
/// sign-in carries, what it is for, the account and the panel it acts on, and its expiry. The
/// raw token is never stored.
#[derive(Debug, Clone, toasty::Model)]
pub struct AuthToken {
    /// Hex-encoded SHA-256 of the token.
    #[key]
    pub token_hash: String,
    /// What the token does when used.
    pub purpose: TokenPurpose,
    /// [`PanelUser::user_id`](super::PanelUser::user_id) of the account the token acts on.
    #[index]
    pub user_id: String,
    /// The prefix of the panel that issued the token: the only panel it works on.
    pub panel: String,
    /// The address a link proves the account reads, for a purpose that proves one.
    pub email: Option<String>,
    #[index]
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
}

/// What an [`AuthToken`] does when used. Stored as its `snake_case` name in a text column, so a
/// purpose a later release adds needs no migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
#[non_exhaustive]
pub enum TokenPurpose {
    /// A mailed link that sets a new password.
    PasswordReset,
    /// A mailed link that proves the account reads the address it was mailed to.
    EmailVerification,
    /// A link mailed to a new address that makes it the account's.
    EmailChange,
    /// A link mailed to the old address that cancels a pending change.
    EmailChangeCancel,
}

/// A new token for `purpose` on the request's panel, held by `user_id` until `lifetime` passes,
/// written through `ex`. The account's earlier tokens for the purpose on that panel end; expired
/// ones wait for [`sweep_expired`].
pub(super) async fn issue(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    purpose: TokenPurpose,
    user_id: &str,
    email: Option<String>,
    lifetime: Duration,
) -> topcoat::Result<Token> {
    let token = Token::random();
    let now = Timestamp::now();
    let expires_at = Timestamp::try_from(std::time::SystemTime::now() + lifetime)
        .map_err(topcoat::Error::from)?;
    let panel = panel_prefix(cx);
    AuthToken::filter(
        AuthToken::fields()
            .user_id()
            .eq(user_id.to_string())
            .and(AuthToken::fields().panel().eq(panel.clone()))
            .and(AuthToken::fields().purpose().eq(purpose)),
    )
    .delete()
    .exec(&mut *ex)
    .await
    .map_err(infrastructure_failure)?;
    toasty::create!(AuthToken {
        token_hash: token_key(&token.hash()),
        purpose,
        user_id: user_id.to_string(),
        panel,
        email,
        expires_at,
        created_at: now,
    })
    .exec(ex)
    .await
    .map_err(infrastructure_failure)?;
    Ok(token)
}

/// The live token for `purpose` that `encoded` names on the request's panel, or `None` for an
/// unknown, expired, malformed or other panel's one.
pub(super) async fn live(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    purpose: TokenPurpose,
    encoded: &str,
) -> topcoat::Result<Option<AuthToken>> {
    let Ok(token) = Token::decode(encoded) else {
        return Ok(None);
    };
    let row = AuthToken::filter(
        AuthToken::fields()
            .token_hash()
            .eq(token_key(&token.hash())),
    )
    .first()
    .exec(ex)
    .await
    .map_err(infrastructure_failure)?;
    let prefix = panel_prefix(cx);
    Ok(row.filter(|row| {
        row.purpose == purpose && row.panel == prefix && row.expires_at > Timestamp::now()
    }))
}

/// Deletes `row` through `ex`: a used token works once.
pub(super) async fn spend(ex: &mut dyn toasty::Executor, row: &AuthToken) -> topcoat::Result<()> {
    AuthToken::filter(AuthToken::fields().token_hash().eq(row.token_hash.clone()))
        .delete()
        .exec(ex)
        .await
        .map_err(infrastructure_failure)
}

/// Deletes every token `user_id` holds on the panels `prefixes` names, or on every panel for
/// `None`, through `ex`: what a credential change ends.
pub(super) async fn end_all(
    ex: &mut dyn toasty::Executor,
    user_id: &str,
    prefixes: Option<Vec<String>>,
) -> topcoat::Result<()> {
    let user = AuthToken::fields().user_id().eq(user_id.to_string());
    match prefixes {
        Some(prefixes) => {
            AuthToken::filter(user.and(AuthToken::fields().panel().in_list(prefixes)))
        }
        None => AuthToken::filter(user),
    }
    .delete()
    .exec(ex)
    .await
    .map_err(infrastructure_failure)
}

/// How many expired token rows one sweep drops: the bound that keeps one login from deleting an
/// unbounded number of rows.
pub(super) const TOKEN_SWEEP_BATCH: usize = 500;

/// Drops up to [`TOKEN_SWEEP_BATCH`] expired token rows when a login succeeds on a panel that
/// issues tokens, outside any transaction: inside one, every issue would contend on the same rows.
pub(super) async fn sweep_expired(cx: &Cx) -> topcoat::Result<()> {
    // A panel that issues no token may leave the model unregistered.
    if current(cx).is_some_and(|panel| panel.auth.issues_tokens()) {
        sweep(&mut crate::db::db(cx)).await?;
    }
    Ok(())
}

/// Drops up to [`TOKEN_SWEEP_BATCH`] expired token rows from `db`.
pub(super) async fn sweep(db: &mut toasty::Db) -> topcoat::Result<()> {
    let now = Timestamp::now();
    let expired: Vec<String> = AuthToken::filter(AuthToken::fields().expires_at().le(now))
        .limit(TOKEN_SWEEP_BATCH)
        .exec(&mut *db)
        .await
        .map_err(infrastructure_failure)?
        .into_iter()
        .map(|row| row.token_hash)
        .collect();
    if expired.is_empty() {
        return Ok(());
    }
    // The expiry is re-checked: the select and the delete are two statements.
    AuthToken::filter(
        AuthToken::fields()
            .token_hash()
            .in_list(expired)
            .and(AuthToken::fields().expires_at().le(now)),
    )
    .delete()
    .exec(db)
    .await
    .map_err(infrastructure_failure)
}

/// A serializable transaction on `db`: two requests racing on one token, or one account, then
/// cannot both commit.
pub(super) async fn serializable(db: &mut toasty::Db) -> topcoat::Result<toasty::Transaction<'_>> {
    db.transaction_builder()
        .isolation(IsolationLevel::Serializable)
        .begin()
        .await
        .map_err(infrastructure_failure)
}
