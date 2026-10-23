//! What auth knows of the panels it serves: each one's [`Realm`], the [`CurrentRealm`] a request
//! to a panel carries, and the [`Host`] the panel layer installs to answer the rest — the realms
//! beside it, and the shell auth's pages render in.
//!
//! Auth names no panel type. It reads a panel through its realm and renders through `Host`, so
//! `auth` sits below `panel` with no edge back (ADR-0024).

use std::sync::Arc;

use topcoat::{context::Cx, view::BoxView};

use super::Auth;
use crate::{
    BoxFuture,
    form::{FieldErrors, FormValues},
    schema::Schema,
};

/// One panel as auth serves it: the prefix its sessions, links and factors are bound to, and the
/// settings its sign-in reads. The bell's flag rides along so that `notification`, which sits
/// below `panel` too, reads it here.
pub(crate) struct Realm {
    /// The panel's prefix, e.g. `/admin`. A session, a mailed link or a factor auth stores names
    /// it, and serves only this realm.
    pub(crate) prefix: String,
    pub(crate) auth: Auth,
    /// The note under the login card, set by [`Panel::login_hint`](crate::Panel::login_hint).
    pub(crate) login_hint: Option<crate::Text>,
    /// The zone a signed-in user without one of their own reads dates in, set by
    /// [`Panel::time_zone`](crate::Panel::time_zone).
    pub(crate) time_zone: jiff::tz::TimeZone,
    /// The messages the panel renders in each locale it offers, set by
    /// [`Panel::locales`](crate::Panel::locales) and
    /// [`Panel::translations`](crate::Panel::translations).
    pub(crate) catalog: crate::i18n::Catalog,
    /// Whether the topbar shows the bell, set by
    /// [`Panel::notifications`](crate::Panel::notifications).
    pub(crate) notifications: bool,
}

impl Realm {
    /// Whether the realm requires a signed-in user.
    pub(crate) fn gates(&self) -> bool {
        !self.auth.is_disabled()
    }
}

/// The request's realm, as the request context carries it. Whatever enters a panel installs it
/// beside the panel's own context, so a request to the panel and a job that enters its handle
/// read the same realm, whether or not the context holds a [`Host`].
#[derive(Clone)]
pub(crate) struct CurrentRealm(pub(crate) Arc<Realm>);

/// What auth asks of the panel layer that mounts it: the realms beside the request's, and the
/// shell its pages render in. The panel installs its implementation as an [`AuthHost`].
pub(crate) trait Host: Send + Sync {
    /// Every mounted panel's realm, in mount order.
    fn realms(&self, cx: &Cx) -> Vec<Arc<Realm>>;

    /// The realms of the panels that declared the runtime endpoint at `path`, in mount order.
    fn declaring(&self, cx: &Cx, path: &str) -> Vec<Arc<Realm>>;

    /// `cx` as a request to `realm`'s panel; `None` when no mounted panel has `realm`.
    fn enter(&self, cx: &Cx, realm: &Arc<Realm>) -> Option<Cx>;

    /// What a request to the panel answers in place of `error`: the panel's error page for a page
    /// load, else `error`.
    fn answer(&self, cx: &Cx, error: topcoat::Error) -> topcoat::Error;

    /// The request's panel brand name.
    fn brand_name(&self, cx: &Cx) -> String;

    /// The brand as a page outside the shell shows it, above its card.
    fn page_brand<'a>(&self, cx: &'a Cx) -> BoxView<'a>;

    /// A complete document around `body`, with the request's panel shell settings.
    fn document<'a>(
        &self,
        cx: &'a Cx,
        title: String,
        body: BoxView<'a>,
    ) -> BoxFuture<'a, topcoat::Result<BoxView<'a>>>;

    /// `page` in the panel's shell, saved to itself and cancelled to the panel's home.
    fn settings_page<'a: 'f, 'f>(
        &self,
        cx: &'a Cx,
        page: SettingsPage<'f>,
    ) -> BoxFuture<'f, topcoat::Result<BoxView<'a>>>;
}

/// A settings form auth renders in the panel's shell: the profile and the tenant settings.
pub(crate) struct SettingsPage<'f> {
    pub(crate) title: String,
    pub(crate) schema: &'f Schema,
    /// A hidden `(name, value)` control the save carries besides the schema's.
    pub(crate) hidden: Option<(&'static str, String)>,
    pub(crate) values: &'f FormValues,
    pub(crate) errors: &'f FieldErrors,
}

/// The [`Host`] the panel layer installs in the app context.
#[derive(Clone, Copy)]
pub(crate) struct AuthHost(pub(crate) &'static dyn Host);

/// The installed host; `None` outside a router that mounts a panel.
pub(crate) fn host(cx: &Cx) -> Option<&'static dyn Host> {
    topcoat::context::try_app_context::<AuthHost>(cx).map(|AuthHost(host)| *host)
}

/// The installed host, or the declaration error an auth page answers on a context without one,
/// such as a panel handle's.
pub(crate) fn hosted(cx: &Cx) -> topcoat::Result<&'static dyn Host> {
    host(cx).ok_or_else(|| {
        crate::error::declaration("auth renders only on a router that mounts a panel".to_owned())
    })
}

/// The realm of the request's panel; `None` outside every panel.
pub(crate) fn current_realm(cx: &Cx) -> Option<&Arc<Realm>> {
    topcoat::context::try_request_context::<CurrentRealm>(cx).map(|CurrentRealm(realm)| realm)
}

/// Every mounted panel's realm, in mount order; none outside a router.
pub(crate) fn realms(cx: &Cx) -> Vec<Arc<Realm>> {
    host(cx).map(|host| host.realms(cx)).unwrap_or_default()
}

/// The request's panel prefix, else the one the request path names.
pub(crate) fn panel_prefix(cx: &Cx) -> String {
    current_realm(cx).map_or_else(
        || crate::naming::path_prefix(topcoat::router::request::uri(cx).path()),
        |realm| realm.prefix.clone(),
    )
}

#[cfg(test)]
mod tests;
