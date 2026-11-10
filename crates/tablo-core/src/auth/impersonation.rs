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
    icon::icon,
    router::{
        Body, RouteFuture,
        error::{forbidden, see_other},
        request::uri,
        response::IntoResponse,
    },
    view::{BoxView, ViewExt, attributes, view},
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
///     // Support acts as customers, never as another member of support.
///     Impersonation::new(|_cx, actor: &AdminUser, target: &AdminUser| {
///         is_support(actor) && !is_support(target)
///     })
///     .lifetime(std::time::Duration::from_secs(30 * 60)),
/// );
/// ```
pub struct Impersonation<U> {
    allows: Arc<Rule<U>>,
    lifetime: Duration,
    across_tenants: bool,
}

/// The longest an [`Impersonation`] lasts, whatever its [`lifetime`](Impersonation::lifetime).
const LONGEST: Duration = Duration::from_hours(24);

impl<U: PanelUser> Impersonation<U> {
    /// Lets a user act as another when `allows(cx, actor, target)` answers `true`, for an hour.
    ///
    /// It is asked again on every request while the actor acts as the target, so a change to
    /// either user, or to the rule, ends it at once. A user never acts as themselves, nor as a
    /// user who may not enter the panel, whatever it answers; on a panel that acts for tenants,
    /// nor as a user who belongs to a tenant the actor does not, unless
    /// [`across_tenants`](Self::across_tenants) says otherwise.
    ///
    /// Decide from `actor` and `target` alone: `cx` carries the request, but whom
    /// [`auth::user`](crate::auth::user) names inside it depends on when the rule is asked.
    pub fn new(allows: impl Fn(&Cx, &U, &U) -> bool + Send + Sync + 'static) -> Self {
        Self {
            allows: Arc::new(allows),
            lifetime: Duration::from_hours(1),
            across_tenants: false,
        }
    }

    /// How long acting as a user lasts before the actor is themselves again; defaults to an hour,
    /// and never exceeds a day.
    #[must_use]
    pub fn lifetime(mut self, lifetime: Duration) -> Self {
        self.lifetime = lifetime.min(LONGEST);
        self
    }

    /// Let the actor act as a user of tenants they are no member of, such as a platform's own
    /// staff acting as any customer. Off by default: on a panel that acts for tenants, every
    /// tenant the target belongs to must be one of the actor's, so acting as them reaches no
    /// tenant the actor could not already.
    #[must_use]
    pub fn across_tenants(mut self) -> Self {
        self.across_tenants = true;
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
    ///   target, on every write made meanwhile. `impersonate` names, as its `until` change, when it
    ///   ends at the latest. Stop, a sign-out, and the first request past the end or after the rule
    ///   refuses record `stop_impersonating`; a session that ends unseen records none, and its
    ///   `until` says when it ended.
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
            across_tenants: impersonation.across_tenants,
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
    across_tenants: bool,
}

impl std::fmt::Debug for ImpersonationConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Impersonation")
            .field("lifetime", &self.lifetime)
            .field("across_tenants", &self.across_tenants)
            .finish_non_exhaustive()
    }
}

impl ImpersonationConfig {
    /// Whether `actor` may act as `target` on `panel`: two users, both admitted to it, the target
    /// in no tenant the actor is not, on a panel that acts for tenants, and the app's rule
    /// agreeing.
    fn permits(
        &self,
        cx: &Cx,
        panel: &Realm,
        actor: &dyn PanelUser,
        target: &dyn PanelUser,
    ) -> bool {
        let within_tenants = !panel.serves_tenants
            || self.across_tenants
            || target.tenants().iter().all(|membership| {
                actor
                    .tenants()
                    .iter()
                    .any(|own| own.tenant == membership.tenant)
            });
        actor.user_id() != target.user_id()
            && actor.can_access_panel(&panel.prefix)
            && target.can_access_panel(&panel.prefix)
            && within_tenants
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
    // Ended by its lifetime, the rule or either user: cleared and recorded together.
    let mut db = crate::db::db(cx)?;
    let mut tx = db.transaction().await.map_err(infrastructure_failure)?;
    session::set_impersonation(&mut tx, &row.token_hash, None).await?;
    if let Some(target) = row.impersonating.as_deref() {
        let tenant = row.tenant;
        record(
            cx,
            &mut tx,
            panel,
            tenant,
            "stop_impersonating",
            &*actor,
            target,
            None,
            None,
        )
        .await?;
    }
    tx.commit().await.map_err(infrastructure_failure)?;
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
    // An actor who owes a step, such as setting up a factor the panel now requires, goes to it.
    if until <= Timestamp::now() || super::gate::owes_step(cx, panel, actor).await? {
        return Ok(None);
    }
    let target = authenticator
        .find_by_id(cx, id)
        .await
        .map_err(infrastructure_failure)?;
    Ok(target.filter(|target| config.permits(cx, panel, actor, &**target)))
}

/// The signed-in user who may start acting as another, with the panel's rule: one acting as
/// themselves, on a panel with [`Auth::impersonation`].
fn actor(cx: &Cx) -> Option<(&SignedIn, &ImpersonationConfig)> {
    let signed = signed(cx).filter(|signed| signed.impersonator.is_none())?;
    Some((signed, signed.realm.auth.impersonation()?))
}

/// A button that acts as `target`, for a page of the app's own, such as a staff list over a user
/// type that is no model, where the [`Impersonate`](crate::Impersonate) action cannot go: a form
/// that posts `target`'s [`user_id`](PanelUser::user_id) to `{prefix}/impersonate` with the CSRF
/// field.
///
/// It renders nothing unless the signed-in user may act as `target` under the panel's
/// [`Impersonation`] rule, as the action's button shows only then; the route asks the rule again.
///
/// ```rust
/// # use tablo_core::{PanelUser, auth};
/// # use topcoat::{context::Cx, view::{View, view}};
/// fn act_as<'a>(cx: &'a Cx, colleague: &dyn PanelUser) -> impl View + 'a {
///     let button = auth::impersonate_button(cx, colleague);
///     view! { <td>(button)</td> }
/// }
/// ```
pub fn impersonate_button<'a>(cx: &'a Cx, target: &dyn PanelUser) -> BoxView<'a> {
    if !may_impersonate(cx, target) {
        return view! {}.boxed();
    }
    let token = crate::csrf::ensure_token(cx);
    let action = format!("{}/impersonate", panel_prefix(cx));
    let id = target.user_id();
    view! {
        <form method="post" action=(action) data-impersonate=(id.clone())>
            (crate::csrf::field(cx, &token))
            <input type="hidden" name=(IMPERSONATE_FIELD) value=(id)>
            tablo_ui::button(
                variant: tablo_ui::ButtonVariant::Outline,
                size: tablo_ui::ButtonSize::Sm,
                attrs: attributes! { type="submit" },
                icon(data: tablo_ui::icons::EYE)
                (crate::t!(cx, "tablo-impersonation-start"))
            )
        </form>
    }
    .boxed()
}

/// Whether the signed-in user may act as `target`: what shows the
/// [`Impersonate`](crate::Impersonate) button.
pub(crate) fn may_impersonate(cx: &Cx, target: &dyn PanelUser) -> bool {
    actor(cx)
        .is_some_and(|(signed, config)| config.permits(cx, &signed.realm, &*signed.user, target))
}

/// Makes the request's session act as `target` through `ex`, and records it through `ex` too, so
/// a transaction commits both or neither; 403 when the signed-in user may not.
pub(crate) async fn start(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    target: &dyn PanelUser,
) -> topcoat::Result<()> {
    let Some((signed, config)) = actor(cx) else {
        return Err(forbidden().into());
    };
    if !config.permits(cx, &signed.realm, &*signed.user, target) {
        return Err(forbidden().into());
    }
    let until =
        Timestamp::try_from(SystemTime::now() + config.lifetime).map_err(topcoat::Error::from)?;
    session::impersonate(cx, &mut *ex, Some((target.user_id(), until))).await?;
    let (realm, tenant) = (&signed.realm, crate::tenant_id(cx));
    let (id, name) = (target.user_id(), Some(target.display_name()));
    record(
        cx,
        ex,
        realm,
        tenant,
        "impersonate",
        &*signed.user,
        &id,
        name,
        Some(until),
    )
    .await?;
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
        let mut db = crate::db::db(cx)?;
        let mut tx = db.transaction().await.map_err(infrastructure_failure)?;
        start(cx, &mut tx, &*target).await?;
        tx.commit().await.map_err(infrastructure_failure)?;
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
            let mut tx = db.transaction().await.map_err(infrastructure_failure)?;
            session::impersonate(cx, &mut tx, None).await?;
            let (realm, tenant) = (&signed.realm, crate::tenant_id(cx));
            let (id, name) = (signed.user.user_id(), Some(signed.user.display_name()));
            let event = "stop_impersonating";
            record(cx, &mut tx, realm, tenant, event, &**actor, &id, name, None).await?;
            tx.commit().await.map_err(infrastructure_failure)?;
            crate::bus::bus(cx).wake(&signed.realm.prefix, &actor.user_id());
        }
        see_other(panel_root(cx)).into_response(cx)
    })
}

/// Records that `signed` stops acting as another user, when they do, as they sign out: the stop a
/// sign-out ends without the Stop button.
pub(super) async fn stop_on_sign_out(cx: &Cx, signed: &SignedIn) -> topcoat::Result<()> {
    let Some(actor) = &signed.impersonator else {
        return Ok(());
    };
    let mut db = crate::db::db(cx)?;
    let (realm, tenant) = (&signed.realm, crate::tenant_id(cx));
    let (id, name) = (signed.user.user_id(), Some(signed.user.display_name()));
    let event = "stop_impersonating";
    record(cx, &mut db, realm, tenant, event, &**actor, &id, name, None).await
}

/// Where the banner's Stop button posts: `{prefix}/impersonate/stop`.
pub(crate) fn stop_url(cx: &Cx) -> String {
    format!("{}/impersonate/stop", panel_prefix(cx))
}

/// Records `event`, done by `actor` to the user of `target_id`, named `target_name` when known, in
/// `panel`'s audit log for `tenant`, when it keeps one; a start names `until`, when it ends on its
/// own.
#[expect(
    clippy::too_many_arguments,
    reason = "one call per event, each naming every part"
)]
async fn record(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    panel: &Realm,
    tenant: Option<crate::TenantId>,
    event: &str,
    actor: &dyn PanelUser,
    target_id: &str,
    target_name: Option<&str>,
    until: Option<Timestamp>,
) -> topcoat::Result<()> {
    if !panel.audit_log {
        return Ok(());
    }
    let mut entry = AuditEvent::new(event)
        .key(target_id.to_owned(), None)
        .change(
            "user",
            crate::t!(cx, "tablo-impersonation-user"),
            None,
            Some(target_name.unwrap_or(target_id).to_owned()),
        );
    // A session that runs out unseen records no stop, so a start names when it ends at the
    // latest.
    if let Some(until) = until {
        entry = entry.change(
            "until",
            crate::t!(cx, "tablo-impersonation-until"),
            None,
            Some(until.to_string()),
        );
    }
    entry
        .write(
            ex,
            panel.prefix.clone(),
            tenant,
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
