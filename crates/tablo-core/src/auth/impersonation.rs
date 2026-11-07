//! [`Auth::impersonation`]: a panel user acting as another, to see the panel as they see it, for a
//! short while and never as far as their credentials.

use std::{
    any::Any,
    sync::Arc,
    time::{Duration, SystemTime},
};

use jiff::Timestamp;
use topcoat::{
    context::Cx,
    router::{
        Body, RouteFuture,
        error::{forbidden, see_other},
        request::uri,
        response::IntoResponse,
    },
};

use super::{
    Auth, AuthConfig, DynAuthenticator, PanelUser, SignedIn, infrastructure_failure, panel_root,
    realm::{Realm, panel_prefix},
    session::{self, AuthSession},
    signed, unauthenticated_error,
};
use crate::AuditEvent;

/// The field of a `POST {prefix}/impersonate` form naming the user to act as: their
/// [`PanelUser::user_id`].
pub const IMPERSONATE_FIELD: &str = "user";

/// The rule an [`Impersonation`] asks: whether `actor` may act as `target`.
type Rule<U> = dyn Fn(&Cx, &U, &U) -> bool + Send + Sync;

/// Who a panel's users may act as, and for how long, set by [`Auth::impersonation`].
///
/// ```rust
/// # use tablo_core::{Auth, Impersonation, auth::AdminUser};
/// # fn is_support(user: &AdminUser) -> bool { user.email.ends_with("@support.example") }
/// let auth = Auth::password().impersonation(
///     Impersonation::new(|_cx, actor: &AdminUser, _target: &AdminUser| is_support(actor))
///         .lifetime(std::time::Duration::from_secs(30 * 60)),
/// );
/// ```
pub struct Impersonation<U> {
    allows: Arc<Rule<U>>,
    lifetime: Duration,
}

impl<U: PanelUser> Impersonation<U> {
    /// Lets a user act as another when `allows(cx, actor, target)` answers `true`, for an hour.
    ///
    /// It is asked again on every request while the actor acts as the target, so a change to
    /// either user, or to the rule, ends it at once. A user never acts as themselves, nor as a
    /// user who may not enter the panel, whatever it answers. `cx` signs in neither user:
    /// [`auth::user`](crate::auth::user) answers `None` inside it.
    pub fn new(allows: impl Fn(&Cx, &U, &U) -> bool + Send + Sync + 'static) -> Self {
        Self {
            allows: Arc::new(allows),
            lifetime: Duration::from_hours(1),
        }
    }

    /// How long acting as a user lasts before the actor is themselves again; defaults to an hour.
    #[must_use]
    pub fn lifetime(mut self, lifetime: Duration) -> Self {
        self.lifetime = lifetime;
        self
    }
}

impl<U: PanelUser> Auth<U> {
    /// Lets a signed-in user act as another user of the panel, as [`Impersonation`] allows: to see
    /// what they see, and to reproduce what they report.
    ///
    /// - `POST {prefix}/impersonate`, carrying the CSRF field and [`IMPERSONATE_FIELD`], starts it,
    ///   and [`Impersonate`](crate::Impersonate) places a button for it on a users resource. It
    ///   answers 403 to a user already acting as another, and for a target the rule refuses or no
    ///   account matches, alike.
    /// - Every page then reads the target as the signed-in user, tenant included, under a banner
    ///   naming them with a Stop button, which posts to `{prefix}/impersonate/stop` and restores
    ///   the actor. So does the end of the lifetime, or the rule refusing again.
    /// - The profile, two-factor, tenant settings, tenant registration and email-verification pages
    ///   answer 403 while it lasts, and an email-change link ignores the session: the actor never
    ///   changes the target's credentials.
    /// - A panel that keeps an [audit log](crate::Panel::audit_log) records `impersonate` and
    ///   `stop_impersonating` for the actor, keyed by the target's id, and names the actor, not the
    ///   target, on every write made meanwhile.
    ///
    /// The session row stores the target in two columns, see [`AuthSession`]. Mounting refuses it
    /// behind [`Auth::external`].
    #[must_use]
    pub fn impersonation(mut self, impersonation: Impersonation<U>) -> Self {
        let allows = impersonation.allows;
        self.config.impersonation = Some(ImpersonationConfig {
            allows: Arc::new(move |cx, actor, target| {
                let (actor, target): (&dyn Any, &dyn Any) = (actor, target);
                match (actor.downcast_ref::<U>(), target.downcast_ref::<U>()) {
                    (Some(actor), Some(target)) => allows(cx, actor, target),
                    _ => false,
                }
            }),
            lifetime: impersonation.lifetime,
        });
        self
    }
}

type Allows = dyn Fn(&Cx, &dyn PanelUser, &dyn PanelUser) -> bool + Send + Sync;

/// An [`Impersonation`] with its user type erased.
#[derive(Clone)]
pub(crate) struct ImpersonationConfig {
    allows: Arc<Allows>,
    lifetime: Duration,
}

impl std::fmt::Debug for ImpersonationConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Impersonation")
            .field("lifetime", &self.lifetime)
            .finish_non_exhaustive()
    }
}

impl ImpersonationConfig {
    /// Whether `actor` may act as `target` on the panel at `prefix`: two users, both admitted to
    /// it, and the app's rule agreeing.
    fn permits(
        &self,
        cx: &Cx,
        prefix: &str,
        actor: &dyn PanelUser,
        target: &dyn PanelUser,
    ) -> bool {
        actor.user_id() != target.user_id()
            && actor.can_access_panel(prefix)
            && target.can_access_panel(prefix)
            && (self.allows)(cx, actor, target)
    }
}

impl AuthConfig {
    /// The impersonation rule, or `None` when no user may act as another.
    pub(crate) fn impersonation(&self) -> Option<&ImpersonationConfig> {
        self.authenticator().and(self.impersonation.as_ref())
    }
}

/// The user acting as the signed-in user, as `U`; `None` while the signed-in user acts as
/// themselves, or when the actor is not a `U`.
pub fn impersonator<U: PanelUser>(cx: &Cx) -> Option<&U> {
    let actor: &dyn Any = &**signed(cx)?.impersonator.as_ref()?;
    actor.downcast_ref()
}

/// Resolves a session whose `actor` acts as another user: the target, with the actor beside, while
/// the lifetime lasts and the rule still allows it; else the actor, the row cleared.
pub(super) async fn resolve(
    cx: &Cx,
    row: AuthSession,
    actor: Arc<dyn PanelUser>,
    panel: &Arc<Realm>,
    authenticator: &dyn DynAuthenticator,
) -> topcoat::Result<SignedIn> {
    if let Some(target) = live_target(cx, &row, &*actor, panel, authenticator).await? {
        return Ok(SignedIn {
            user: target,
            realm: Arc::clone(panel),
            tenant: row.tenant,
            impersonator: Some(actor),
        });
    }
    session::set_impersonation(&mut crate::db::db(cx)?, &row.token_hash, None).await?;
    Ok(SignedIn {
        user: actor,
        realm: Arc::clone(panel),
        tenant: None,
        impersonator: None,
    })
}

async fn live_target(
    cx: &Cx,
    row: &AuthSession,
    actor: &dyn PanelUser,
    panel: &Realm,
    authenticator: &dyn DynAuthenticator,
) -> topcoat::Result<Option<Arc<dyn PanelUser>>> {
    let (Some(id), Some(until), Some(config)) = (
        row.impersonating.as_deref(),
        row.impersonating_until,
        panel.auth.impersonation(),
    ) else {
        return Ok(None);
    };
    if until <= Timestamp::now() {
        return Ok(None);
    }
    let target = authenticator
        .find_by_id(cx, id)
        .await
        .map_err(infrastructure_failure)?;
    Ok(target.filter(|target| config.permits(cx, &panel.prefix, actor, &**target)))
}

/// The signed-in user who may start acting as another, with the panel's rule: one acting as
/// themselves, on a panel with [`Auth::impersonation`].
fn actor(cx: &Cx) -> Option<(&SignedIn, &ImpersonationConfig)> {
    let signed = signed(cx).filter(|signed| signed.impersonator.is_none())?;
    Some((signed, signed.realm.auth.impersonation()?))
}

/// Whether the signed-in user may act as `target`: what shows the
/// [`Impersonate`](crate::Impersonate) button.
pub(crate) fn may_impersonate(cx: &Cx, target: &dyn PanelUser) -> bool {
    actor(cx).is_some_and(|(signed, config)| {
        config.permits(cx, &signed.realm.prefix, &*signed.user, target)
    })
}

/// Makes the request's session act as `target` through `ex`, and records it; 403 when the
/// signed-in user may not.
pub(crate) async fn start(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    target: &dyn PanelUser,
) -> topcoat::Result<()> {
    let Some((signed, config)) = actor(cx) else {
        return Err(forbidden().into());
    };
    if !config.permits(cx, &signed.realm.prefix, &*signed.user, target) {
        return Err(forbidden().into());
    }
    let until =
        Timestamp::try_from(SystemTime::now() + config.lifetime).map_err(topcoat::Error::from)?;
    session::impersonate(cx, &mut *ex, Some((target.user_id(), until))).await?;
    record(cx, ex, &signed.realm, "impersonate", &*signed.user, target).await?;
    crate::bus::bus(cx).wake(&signed.realm.prefix, &signed.user.user_id());
    Ok(())
}

/// `POST {prefix}/impersonate`: act as the user [`IMPERSONATE_FIELD`] names, then land on the
/// panel root.
pub(super) fn start_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let Some(signed) = signed(cx) else {
            return Err(unauthenticated_error(cx));
        };
        let parts = super::input::read_form(cx, body).await?;
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        let authenticator = signed.realm.auth.authenticator().ok_or_else(forbidden)?;
        let id = parts.last(IMPERSONATE_FIELD).ok_or_else(forbidden)?;
        // An unknown id answers as a refused one does, so the route tells no one who exists.
        let target = authenticator
            .find_by_id(cx, id)
            .await
            .map_err(infrastructure_failure)?
            .ok_or_else(forbidden)?;
        start(cx, &mut crate::db::db(cx)?, &*target).await?;
        see_other(panel_root(cx)).into_response(cx)
    })
}

/// `POST {prefix}/impersonate/stop`: the actor is themselves again, on the panel root.
pub(super) fn stop_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let Some(signed) = signed(cx) else {
            return Err(unauthenticated_error(cx));
        };
        let parts = super::input::read_form(cx, body).await?;
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        if let Some(actor) = &signed.impersonator {
            let mut db = crate::db::db(cx)?;
            session::impersonate(cx, &mut db, None).await?;
            record(
                cx,
                &mut db,
                &signed.realm,
                "stop_impersonating",
                &**actor,
                &*signed.user,
            )
            .await?;
            crate::bus::bus(cx).wake(&signed.realm.prefix, &actor.user_id());
        }
        see_other(panel_root(cx)).into_response(cx)
    })
}

/// Where the banner's Stop button posts: `{prefix}/impersonate/stop`.
pub(crate) fn stop_url(cx: &Cx) -> String {
    format!("{}/impersonate/stop", panel_prefix(cx))
}

/// Records `event`, done by `actor` to `target`, in `panel`'s audit log when it keeps one.
async fn record(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    panel: &Realm,
    event: &str,
    actor: &dyn PanelUser,
    target: &dyn PanelUser,
) -> topcoat::Result<()> {
    if !panel.audit_log {
        return Ok(());
    }
    AuditEvent::new(event)
        .key(target.user_id(), None)
        .change(
            "user",
            crate::t!(cx, "tablo-impersonation-user"),
            None,
            Some(target.display_name().to_owned()),
        )
        .write(
            ex,
            panel.prefix.clone(),
            crate::tenant_id(cx),
            Some((actor.user_id(), actor.display_name().to_owned())),
        )
        .await
        .map_err(|error| {
            crate::error::driver_failure(error, "The audit log is unavailable right now.")
        })?;
    Ok(())
}

/// Whether the request reaches a page the actor may not use while acting as another user: one
/// that changes the target's credentials, address, factors or tenants.
pub(super) fn refused(cx: &Cx, panel: &Realm) -> bool {
    let path = uri(cx).path();
    let prefix = &panel.prefix;
    [
        panel.pages.profile.clone(),
        panel.pages.two_factor.clone(),
        panel.pages.tenant_profile.clone(),
        format!("{prefix}/register-tenant"),
        format!("{prefix}/verify-email"),
    ]
    .iter()
    .any(|root| {
        path.strip_prefix(root.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}
