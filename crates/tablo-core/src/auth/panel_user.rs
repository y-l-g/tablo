//! [`PanelUser`] and the [`Authenticator`] that loads one.

use std::{any::Any, future::Future};

use topcoat::context::Cx;

use crate::tenancy::{Membership, TenantId};

/// A signed-in user, as the panel knows it (ADR-0013).
///
/// Implement it for the app's own user type and return that type from an
/// [`Authenticator`]; the shipped [`AdminUser`](super::AdminUser) implements it too. App code
/// reads the user back with its own type through [`user`](super::user).
///
/// ```rust
/// # struct Staff {
/// #     id: uuid::Uuid,
/// #     name: String,
/// #     active: bool,
/// #     memberships: Vec<tablo_core::Membership>,
/// # }
/// # use tablo_core::{Membership, PanelUser};
/// impl PanelUser for Staff {
///     fn user_id(&self) -> String {
///         self.id.to_string()
///     }
///     fn display_name(&self) -> &str {
///         &self.name
///     }
///     fn can_access_panel(&self, _panel: &str) -> bool {
///         self.active
///     }
///     fn tenants(&self) -> &[Membership] {
///         &self.memberships
///     }
/// }
/// ```
pub trait PanelUser: Any + Send + Sync {
    /// The stable key the session stores and
    /// [`Authenticator::find_by_id`] receives. Any string fits.
    fn user_id(&self) -> String;

    /// The name the user menu shows.
    fn display_name(&self) -> &str;

    /// The muted line under the name in the user menu, such as an email or a role. Defaults to
    /// none.
    fn subtitle(&self) -> Option<&str> {
        None
    }

    /// The address the account stores, which mail meant for the account goes to: a reset link,
    /// a verification link, a sign-in code. Defaults to none: the account gets no mail.
    fn email(&self) -> Option<&str> {
        None
    }

    /// Whether the user may enter the panel at `panel`, its prefix with one leading slash and
    /// none trailing: `/admin` for `Panel::new("admin")`. A user who may not answers 403,
    /// indistinguishable from bad credentials at login. Every user type answers it: a sign-up on
    /// one panel writes an account every panel over the same user type looks up.
    ///
    /// One user type can serve several panels, each with its own rule. Name each panel and refuse
    /// the rest, so that a panel added or renamed later admits nobody until it has a rule:
    ///
    /// ```rust,ignore
    /// fn can_access_panel(&self, panel: &str) -> bool {
    ///     match panel {
    ///         "/admin" => self.active && self.is_staff,
    ///         "/portal" => self.active,
    ///         _ => false,
    ///     }
    /// }
    /// ```
    ///
    /// Every check of a panel asks it with that panel's prefix: the gate on each request, the
    /// login, the sign-up, a mailed link and a two-factor challenge. The link that cancels an
    /// email change does not ask: it only restores the old address and ends sessions.
    fn can_access_panel(&self, panel: &str) -> bool;

    /// The tenants the user may act for, in the order the tenant switcher
    /// lists them. A session acts for the [`default_tenant`](Self::default_tenant)
    /// until the user selects another. Defaults to none: the user has no tenant.
    ///
    /// A panel acts for tenants when it offers a tenant registration or settings page, or mounts
    /// a resource over tenant-owned rows. Any other acts for no tenant:
    /// [`tenant_id`](crate::tenant_id) answers `None` there, and the panel offers no tenant
    /// menu and no route to switch.
    ///
    /// With two or more, on a panel that acts for tenants, a tenant menu sits atop the sidebar. It
    /// posts to `{prefix}/tenant`, which stores the choice on the session and returns to the
    /// panel's home page. A tenant the user is not a member of answers 403, and a stored choice
    /// applies only while the membership lasts. One browser acts for one tenant at a time, in
    /// every tab.
    fn tenants(&self) -> &[Membership] {
        &[]
    }

    /// The tenant a session acts for until the user selects another, while it is one of
    /// [`tenants`](Self::tenants), such as one the user picks on their profile page and the app
    /// stores. Defaults to none: the first of `tenants`.
    fn default_tenant(&self) -> Option<TenantId> {
        None
    }

    /// The time zone the user reads and enters wall-clock times in: a timestamp's control, its
    /// [`DateColumn`](crate::DateColumn) day and its [`DateFilter`](crate::DateFilter) days.
    /// Defaults to none: the panel's [`time_zone`](crate::Panel::time_zone).
    fn time_zone(&self) -> Option<jiff::tz::TimeZone> {
        None
    }

    /// The locale the user reads the panel and their mails in, such as `fr`. Defaults to none: the
    /// browser's best match among the panel's [`locales`](crate::Panel::locales), else its
    /// default. A locale matches the closest the panel offers, as `fr-CA` reads `fr`; one with no
    /// match counts as none.
    ///
    /// A user who stores their locale as text parses it:
    ///
    /// ```rust,ignore
    /// fn locale(&self) -> Option<Locale> {
    ///     self.locale.as_deref()?.parse().ok()
    /// }
    /// ```
    fn locale(&self) -> Option<topcoat::l10n::Locale> {
        None
    }
}

/// How a panel loads its users (ADR-0013).
///
/// The default [`PasswordAuth`](crate::PasswordAuth) implements it against the shipped
/// [`AdminUser`](super::AdminUser) model; an app with an existing user table implements it and
/// passes the value to [`Panel::auth`](crate::Panel::auth) via
/// [`Auth::custom`](crate::Auth::custom). Sessions are the framework's, so an implementation only
/// maps credentials to a user and a stored id back to one.
///
/// Every *credential* failure must return `Ok(None)`, never a distinguishable
/// error: the login response is one generic message for all of them. An
/// infrastructure failure is not a credential verdict, so an implementation
/// that cannot reach its store returns the driver's error instead — the login
/// handler maps it to the opaque outage page, which keeps a database outage
/// from rendering as a rejected password. An implementation's own error keeps
/// its own mapping.
pub trait Authenticator: Send + Sync + 'static {
    /// The app's user type.
    type User: PanelUser;

    /// Verify `login`/`password`, returning the user on success.
    ///
    /// Implementations must run comparable work for unknown accounts so
    /// timing does not leak account existence; [`verify_password`](super::verify_password) does,
    /// given `None` for an unknown account.
    fn verify(
        &self,
        cx: &Cx,
        login: &str,
        password: &str,
    ) -> impl Future<Output = topcoat::Result<Option<Self::User>>> + Send;

    /// Load the session's user by [`PanelUser::user_id`], with everything the request
    /// reads from it — its [`tenants`](PanelUser::tenants) included.
    ///
    /// It runs on every request, which is what makes deactivation, revocation
    /// and a removed membership take effect immediately; return `None` when
    /// the user no longer authenticates.
    fn find_by_id(
        &self,
        cx: &Cx,
        id: &str,
    ) -> impl Future<Output = topcoat::Result<Option<Self::User>>> + Send;

    /// Whether `password` is `user`'s current password: what a page asks before a change a stolen
    /// session must not make, such as turning two-factor sign-in on.
    ///
    /// It defaults to [`verify`](Self::verify) with the user's [`email`](PanelUser::email) as the
    /// login, and answers `false` for a user without one: override it when users sign in with
    /// something else.
    fn confirm_password(
        &self,
        cx: &Cx,
        user: &Self::User,
        password: &str,
    ) -> impl Future<Output = topcoat::Result<bool>> + Send {
        async move {
            let Some(login) = user.email() else {
                return Ok(false);
            };
            let verified = self.verify(cx, login, password).await?;
            Ok(verified.is_some_and(|found| found.user_id() == user.user_id()))
        }
    }
}
