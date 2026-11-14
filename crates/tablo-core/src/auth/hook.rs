//! [`AuthHook`]: the work a panel does when auth changes who a request acts as.

use std::future::Future;

use jiff::Timestamp;
use toasty::Executor;
use topcoat::{Result, context::Cx};

use super::{PanelUser, SignedIn, infrastructure_failure, realm::Realm};
use crate::{BoxFuture, PanelId, TenantId};

/// Work a panel does inside the transaction of each [`AuthEvent`], registered by
/// [`Panel::auth_hook`](crate::Panel::auth_hook): an audit trail of who acted as whom.
///
/// Run every statement through `ex`. An `Err` on an event that grants or changes something (a
/// sign-in, a start of impersonation, a change of credentials, address or second factor) rolls it
/// back and fails the request. An `Err` on a stop of impersonation, a sign-out or a refused
/// attempt rolls back what the hooks wrote and is logged, and the event stands: a session that
/// cannot stop acting as someone else, or sign out, is worse than one the trail misses.
///
/// ```rust
/// # use tablo_core::{AuthEvent, AuthEventKind, AuthHook};
/// # use topcoat::{Result, context::Cx};
/// struct Trace;
///
/// impl AuthHook for Trace {
///     async fn on_event(
///         _cx: &Cx,
///         event: &AuthEvent<'_>,
///         _ex: &mut dyn toasty::Executor,
///     ) -> Result<()> {
///         if let (AuthEventKind::Impersonate { .. }, Some(actor), Some(target)) =
///             (event.kind(), event.actor(), event.target_id())
///         {
///             tracing::info!("{} acts as {target}", actor.user_id());
///         }
///         Ok(())
///     }
/// }
/// ```
pub trait AuthHook: 'static {
    /// Work inside the event's transaction.
    fn on_event(
        cx: &Cx,
        event: &AuthEvent<'_>,
        ex: &mut dyn Executor,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// What auth did, as an [`AuthHook`] receives it: on which panel, for which tenant, by whom, to
/// which account.
///
/// The request may not know the user yet: a sign-in has no session before it, and a session that
/// lapsed is stopped while auth resolves it, so read the actor and the tenant here, not through
/// [`auth::actor`](crate::auth::actor) and [`tenant_id`](crate::tenant_id).
#[derive(Clone)]
pub struct AuthEvent<'a> {
    kind: AuthEventKind<'a>,
    panel: &'a PanelId,
    tenant: Option<TenantId>,
    actor: Option<&'a dyn PanelUser>,
    target_id: Option<String>,
    target_name: Option<&'a str>,
}

impl<'a> AuthEvent<'a> {
    /// `actor` did `kind` to the account `target_id` names.
    pub(crate) fn new(
        kind: AuthEventKind<'a>,
        panel: &'a PanelId,
        tenant: Option<TenantId>,
        actor: &'a dyn PanelUser,
        target_id: &str,
        target_name: Option<&'a str>,
    ) -> Self {
        Self {
            kind,
            panel,
            tenant,
            actor: Some(actor),
            target_id: Some(target_id.to_owned()),
            target_name,
        }
    }

    /// `user` did `kind` to their own account.
    pub(crate) fn own(
        kind: AuthEventKind<'a>,
        panel: &'a PanelId,
        tenant: Option<TenantId>,
        user: &'a dyn PanelUser,
    ) -> Self {
        Self {
            kind,
            panel,
            tenant,
            actor: Some(user),
            target_id: Some(user.user_id()),
            target_name: Some(user.display_name()),
        }
    }

    /// `kind` happened to the account `target_id` names with no one signed in: a mailed link
    /// opened without a session, or a sign-in no account answered.
    pub(crate) fn unsigned(
        kind: AuthEventKind<'a>,
        panel: &'a PanelId,
        target_id: Option<&str>,
    ) -> Self {
        Self {
            kind,
            panel,
            tenant: None,
            actor: None,
            target_id: target_id.map(str::to_owned),
            target_name: None,
        }
    }

    /// What happened.
    pub fn kind(&self) -> AuthEventKind<'a> {
        self.kind
    }

    /// The panel it happened on.
    pub fn panel(&self) -> &'a PanelId {
        self.panel
    }

    /// The tenant the session acted for; `None` outside every tenant, and before a session
    /// exists.
    pub fn tenant(&self) -> Option<TenantId> {
        self.tenant
    }

    /// The user who did it; `None` when no one was signed in: a sign-in no account answered, or
    /// a mailed link opened without a session.
    pub fn actor(&self) -> Option<&'a dyn PanelUser> {
        self.actor
    }

    /// The [`PanelUser::user_id`] of the account it was done to, the actor's own outside
    /// impersonation; `None` when a sign-in named no account.
    pub fn target_id(&self) -> Option<&str> {
        self.target_id.as_deref()
    }

    /// The [`display_name`](PanelUser::display_name) of the account it was done to; `None` when
    /// the account was not loaded.
    pub fn target_name(&self) -> Option<&'a str> {
        self.target_name
    }
}

impl std::fmt::Debug for AuthEvent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthEvent")
            .field("kind", &self.kind)
            .field("panel", &self.panel.to_string())
            .field("tenant", &self.tenant)
            .field("actor", &self.actor.map(PanelUser::user_id))
            .field("target_id", &self.target_id)
            .field("target_name", &self.target_name)
            .finish()
    }
}

/// Which [`AuthEvent`] auth recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthEventKind<'a> {
    /// The actor starts acting as the target through
    /// [`Auth::impersonation`](crate::Auth::impersonation), until this time at the latest: a
    /// session that runs out unseen records no stop.
    Impersonate { until: Timestamp },
    /// The actor stops acting as the target: with the banner's Stop button, by signing out, or on
    /// the first request after the lifetime ran out or the rule refused.
    StopImpersonating,
    /// The actor got a session on the panel: by password, by a provider, after their second
    /// factor, by signing up, or from the app's own sign-in under
    /// [`Auth::external`](crate::Auth::external).
    SignIn,
    /// A sign-in the panel refused, as `login` was typed. The actor is the account when its
    /// password was right and it may not enter the panel, else `None`: a wrong password and an
    /// unknown login read alike. An attempt past the throttle records nothing.
    SignInFailed { login: &'a str },
    /// The actor's password was right and the second-factor code they gave was wrong.
    SecondFactorFailed,
    /// The actor signed out of the panel.
    SignOut,
    /// The actor changed something they sign in with, such as their password, on the profile
    /// page: a [`ProfileUpdate::Credentials`](crate::ProfileUpdate::Credentials).
    CredentialsChanged,
    /// The target's password was set through a mailed reset link.
    PasswordReset,
    /// The target's address changed, by the link mailed to the new one.
    EmailChanged,
    /// The actor turned two-factor sign-in on.
    TwoFactorEnabled,
    /// The actor turned two-factor sign-in off.
    TwoFactorDisabled,
    /// The actor replaced their recovery codes.
    RecoveryCodesReplaced,
}

/// A registered [`AuthHook`], its type erased.
pub(crate) type AuthHookFn =
    for<'a> fn(&'a Cx, &'a AuthEvent<'a>, &'a mut dyn Executor) -> BoxFuture<'a, Result<()>>;

/// `H`'s [`AuthHook::on_event`], its type erased.
pub(crate) fn erased<'a, H: AuthHook>(
    cx: &'a Cx,
    event: &'a AuthEvent<'a>,
    ex: &'a mut dyn Executor,
) -> BoxFuture<'a, Result<()>> {
    Box::pin(H::on_event(cx, event, ex))
}

/// Runs each of `hooks` on `event` through `ex`, stopping at the first error.
pub(crate) async fn run(
    cx: &Cx,
    hooks: &[AuthHookFn],
    event: &AuthEvent<'_>,
    ex: &mut dyn Executor,
) -> Result<()> {
    for hook in hooks {
        hook(cx, event, &mut *ex).await.map_err(|error| {
            crate::error::driver_failure(error, crate::error::DATABASE_UNAVAILABLE)
        })?;
    }
    Ok(())
}

/// Runs the hooks of `realm` on `event` in a transaction of its own. A failure is logged, not
/// returned: for an event that stands whether or not the trail holds it.
pub(crate) async fn note(cx: &Cx, realm: &Realm, event: &AuthEvent<'_>) {
    if realm.auth_hooks.is_empty() {
        return;
    }
    let noted = async {
        let mut db = crate::db::db(cx)?;
        let mut tx = db.transaction().await.map_err(infrastructure_failure)?;
        match run(cx, &realm.auth_hooks, event, &mut tx).await {
            Ok(()) => tx.commit().await.map_err(infrastructure_failure),
            Err(error) => {
                tx.rollback().await.map_err(infrastructure_failure)?;
                Err(error)
            }
        }
    }
    .await;
    if let Err(error) = noted {
        tracing::error!(error = %error, ?event, "an auth hook failed to record an event");
    }
}

/// Runs the hooks of the panel `signed` acts on through `ex`, on the `kind` its user did to their
/// own account.
pub(crate) async fn run_own(
    cx: &Cx,
    signed: &SignedIn,
    kind: AuthEventKind<'_>,
    ex: &mut dyn Executor,
) -> Result<()> {
    let event = AuthEvent::own(kind, &signed.realm.id, signed.tenant, &*signed.user);
    run(cx, &signed.realm.auth_hooks, &event, ex).await
}
