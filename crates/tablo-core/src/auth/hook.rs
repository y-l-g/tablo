//! [`AuthHook`]: the work a panel does when auth changes who a request acts as.

use std::future::Future;

use jiff::Timestamp;
use toasty::Executor;
use topcoat::{Result, context::Cx};

use super::PanelUser;
use crate::{BoxFuture, PanelId, TenantId};

/// Work a panel does inside the transaction of each [`AuthEvent`], registered by
/// [`Panel::auth_hook`](crate::Panel::auth_hook): an audit trail of who acted as whom.
///
/// Run every statement through `ex`. An `Err` on [`Impersonate`](AuthEventKind::Impersonate)
/// rolls the start back and fails the request. An `Err` on a stop rolls back what the hooks wrote
/// and is logged, and the stop happens anyway: a session that cannot stop acting as someone else,
/// or sign out, is worse than a stop the trail misses.
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
///         if let AuthEventKind::Impersonate { .. } = event.kind() {
///             tracing::info!("{} acts as {}", event.actor().user_id(), event.target_id());
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
/// whom.
///
/// The request may not know the user yet: a session that lapsed is stopped while auth resolves
/// it, so read the actor and the tenant here, not through [`auth::actor`](crate::auth::actor)
/// and [`tenant_id`](crate::tenant_id).
#[derive(Clone, Copy)]
pub struct AuthEvent<'a> {
    kind: AuthEventKind,
    panel: &'a PanelId,
    tenant: Option<TenantId>,
    actor: &'a dyn PanelUser,
    target_id: &'a str,
    target_name: Option<&'a str>,
}

impl<'a> AuthEvent<'a> {
    pub(crate) fn new(
        kind: AuthEventKind,
        panel: &'a PanelId,
        tenant: Option<TenantId>,
        actor: &'a dyn PanelUser,
        target_id: &'a str,
        target_name: Option<&'a str>,
    ) -> Self {
        Self {
            kind,
            panel,
            tenant,
            actor,
            target_id,
            target_name,
        }
    }

    /// What happened.
    pub fn kind(&self) -> AuthEventKind {
        self.kind
    }

    /// The panel it happened on.
    pub fn panel(&self) -> &'a PanelId {
        self.panel
    }

    /// The tenant the session acted for; `None` outside every tenant.
    pub fn tenant(&self) -> Option<TenantId> {
        self.tenant
    }

    /// The user who did it.
    pub fn actor(&self) -> &'a dyn PanelUser {
        self.actor
    }

    /// The [`PanelUser::user_id`] of the user it was done to.
    pub fn target_id(&self) -> &'a str {
        self.target_id
    }

    /// The [`display_name`](PanelUser::display_name) of the user it was done to; `None` when a
    /// lapsed session's target no longer loads.
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
            .field("actor", &self.actor.user_id())
            .field("target_id", &self.target_id)
            .field("target_name", &self.target_name)
            .finish()
    }
}

/// Which [`AuthEvent`] auth recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthEventKind {
    /// The actor starts acting as the target through
    /// [`Auth::impersonation`](crate::Auth::impersonation), until this time at the latest: a
    /// session that runs out unseen records no stop.
    Impersonate { until: Timestamp },
    /// The actor stops acting as the target: with the banner's Stop button, by signing out, or on
    /// the first request after the lifetime ran out or the rule refused.
    StopImpersonating,
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
