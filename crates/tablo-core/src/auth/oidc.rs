//! Provider sign-in: the [`IdentityLinker`] trait that turns a provider's identity into the app's
//! user, the [`Oidc`] configuration, and the `{prefix}/sign-in/{key}` routes that leave for an
//! OpenID Connect provider and come back from it.

mod linker;
mod provider;

use std::{marker::PhantomData, time::Duration};

use topcoat::{
    context::Cx,
    cookie::{Cookie, Cookies, SameSite, cookies, try_private_cookies},
    router::{
        Body, RouteFuture,
        error::{not_found, see_other},
        request::{client_ip, uri},
        response::{IntoResponse, Response},
    },
    view::{BoxView, ViewExt, view},
};

use self::{
    linker::DynLinker,
    provider::{Failure, Pending},
};
pub use self::{
    linker::{IdentityLinker, ProviderIdentity},
    provider::OidcProvider,
};
use super::{
    Auth, PanelUser, infrastructure_failure,
    login::{admit, login_url, next_from_query},
    mail::private,
    realm::{Realm, current_realm, panel_prefix},
    registration::with_next,
};
use crate::{flash::set_notification, outcome::Notification};

#[cfg(test)]
mod tests;

/// How long a browser has to come back from the provider.
const PENDING_LIFETIME: Duration = Duration::from_mins(10);

/// The cookie a pending sign-in travels in, sealed under the router's cookie `Key`. One per
/// browser: a sign-in started in a second tab replaces the first's.
const COOKIE_NAME: &str = "__Host-tablo-oidc";

/// What the sign-in throttle counts provider callbacks under, apart from password sign-ins.
const THROTTLE_SCOPE: &str = "provider-sign-in";

/// A panel's provider sign-in, installed with [`Auth::oidc`]: a button on the login and sign-up
/// pages for each [`OidcProvider`], which leaves for the provider and comes back signed in as the
/// user the [`IdentityLinker`] names.
///
/// A provider sign-in ends as a password one does: an account with a second factor answers the
/// two-factor challenge, and a new session replaces the browser's. It needs the router's cookie
/// `Key`, which seals the sign-in's secrets while the browser is at the provider.
///
/// ```rust
/// # use tablo_core::{Auth, IdentityLinker, Oidc, OidcProvider, ProviderIdentity, auth::AdminUser};
/// # struct Linker;
/// # impl IdentityLinker for Linker {
/// #     type User = AdminUser;
/// #     async fn sign_in(
/// #         &self,
/// #         _cx: &topcoat::context::Cx,
/// #         _identity: &ProviderIdentity,
/// #         _ex: &mut dyn toasty::Executor,
/// #     ) -> topcoat::Result<Option<AdminUser>> {
/// #         Ok(None)
/// #     }
/// # }
/// let auth = Auth::password().oidc(
///     Oidc::new("https://app.example.com", Linker)
///         .provider(OidcProvider::google("client-id", "client-secret")),
/// );
/// ```
pub struct Oidc<U> {
    /// The scheme and host every redirect URI starts with, without a trailing `/`.
    base_url: String,
    linker: Box<dyn DynLinker>,
    pub(super) providers: Vec<OidcProvider>,
    user: PhantomData<fn() -> U>,
}

impl<U: PanelUser> Oidc<U> {
    /// Signs users in through `linker`, with redirect URIs that start with `base_url`, such as
    /// `https://app.example.com`: the app's public origin, never one a request names. Add each
    /// provider with [`provider`](Self::provider).
    ///
    /// The pending sign-in's cookie belongs to the host the browser started on, and the provider
    /// sends it back to `base_url`'s: serve the panel on that host alone, redirecting any other,
    /// such as `www.`, to it, or a sign-in started there comes back without its cookie and
    /// fails.
    #[must_use]
    pub fn new(base_url: impl Into<String>, linker: impl IdentityLinker<User = U>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            linker: Box::new(linker),
            providers: Vec::new(),
            user: PhantomData,
        }
    }

    /// Adds `provider`'s button, after the providers added before it.
    #[must_use]
    pub fn provider(mut self, provider: OidcProvider) -> Self {
        self.providers.push(provider);
        self
    }

    fn erase(self) -> Oidc<()> {
        Oidc {
            base_url: self.base_url,
            linker: self.linker,
            providers: self.providers,
            user: PhantomData,
        }
    }
}

impl Oidc<()> {
    /// The provider whose key is `key`.
    fn find(&self, key: &str) -> Option<&OidcProvider> {
        self.providers.iter().find(|provider| provider.key == key)
    }

    /// The URI the provider sends the browser back to: `{base_url}{prefix}/sign-in/{key}/callback`.
    fn redirect_uri(&self, prefix: &str, key: &str) -> String {
        format!("{}{prefix}/sign-in/{key}/callback", self.base_url)
    }
}

impl<U> std::fmt::Debug for Oidc<U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Oidc")
            .field("base_url", &self.base_url)
            .field("providers", &self.providers)
            .finish_non_exhaustive()
    }
}

impl<U: PanelUser> Auth<U> {
    /// Adds provider sign-in: a button on the login page, and on the sign-up page when the panel
    /// offers one, for each of `oidc`'s providers. See [`Oidc`].
    ///
    /// A pending sign-in lasts ten minutes, in a cookie sealed under the router's cookie `Key`;
    /// the provider's answer must carry its state, and the ID token its nonce. A failed sign-in,
    /// whether the provider refused, its answer failed a check or the linker returned `None`,
    /// lands on the login page with one message.
    ///
    /// Mounting refuses it on [`Auth::disabled`] and beside [`Auth::external`], on a router
    /// without a cookie `Key`, with a base URL that is not an `https` origin, without a provider,
    /// and with a provider whose key is not lowercase ASCII letters, digits and `-`, whose key
    /// another provider holds, or whose issuer is neither `https` nor `http` on a loopback host.
    #[must_use]
    pub fn oidc(mut self, oidc: Oidc<U>) -> Self {
        self.config.oidc = Some(oidc.erase());
        self
    }
}

/// Where the sign-in through `key` starts: `{prefix}/sign-in/{key}`.
fn start_url(cx: &Cx, key: &str) -> String {
    format!("{}/sign-in/{key}", panel_prefix(cx))
}

/// The provider whose start or callback route `path` is on the request's panel, and whether it is
/// the callback.
fn route_of<'a>(cx: &'a Cx, path: &str) -> Option<(&'a OidcProvider, bool)> {
    let oidc = current_realm(cx)?.auth.oidc()?;
    let rest = path
        .strip_prefix(&panel_prefix(cx))?
        .strip_prefix("/sign-in/")?;
    let (key, callback) = match rest.strip_suffix("/callback") {
        Some(key) => (key, true),
        None => (rest, false),
    };
    Some((oidc.find(key)?, callback))
}

/// Whether `path` is a provider sign-in route of the request's panel, which the gate serves
/// while signed out.
pub(super) fn serves(cx: &Cx, path: &str) -> bool {
    route_of(cx, path).is_some()
}

/// The provider buttons the login and sign-up pages show under their form, carrying `next`;
/// `None` when the panel installs no provider.
pub(super) fn buttons<'a>(cx: &'a Cx, next: &str) -> Option<BoxView<'a>> {
    let oidc = current_realm(cx)?.auth.oidc()?;
    let links: Vec<(String, String, String)> = oidc
        .providers
        .iter()
        .map(|provider| {
            let label = crate::t!(
                cx,
                "tablo-oidc-continue",
                provider = provider.label.localize(cx)
            );
            let href = with_next(start_url(cx, &provider.key), next);
            (provider.key.clone(), href, label)
        })
        .collect();
    let or = crate::t!(cx, "tablo-oidc-or");
    Some(
        view! {
            <div class="flex flex-col gap-3">
                <div
                    class="flex items-center gap-3 text-xs text-muted-foreground uppercase"
                >
                    <span class="h-px flex-1 bg-border"></span>
                    (or)
                    <span class="h-px flex-1 bg-border"></span>
                </div>
                for (key, href, label) in links {
                    // A plain link: the browser leaves for the provider, which the runtime's
                    // fetch and the panel's `form-action` would both stop.
                    <a
                        href=(href)
                        data-oidc-provider=(key)
                        class=(tablo_ui::button_variants(
                            tablo_ui::ButtonVariant::Outline,
                            tablo_ui::ButtonSize::Md,
                        ))
                    >
                        (label)
                    </a>
                }
            </div>
        }
        .boxed(),
    )
}

/// `GET {prefix}/sign-in/{key}` — remember a new sign-in in a sealed cookie and leave for the
/// provider.
pub(super) fn start(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current_realm(cx).ok_or_else(not_found)?;
        let oidc = panel.auth.oidc().ok_or_else(not_found)?;
        let (provider, false) = route_of(cx, uri(cx).path()).ok_or_else(not_found)? else {
            return Err(not_found().into());
        };
        let next = next_from_query(cx).unwrap_or_default();
        let pending = Pending::new(panel.prefix.clone(), provider.key.clone(), next.clone());
        let redirect_uri = oidc.redirect_uri(&panel.prefix, &provider.key);
        let url = match provider.authorization_url(&redirect_uri, &pending).await {
            Ok(url) => url,
            Err(failure) => return refused(cx, provider, failure, &next),
        };
        let sealed = serde_json::to_string(&pending).map_err(topcoat::Error::from)?;
        sealed_cookies(cx)?.add(
            Cookie::build((COOKIE_NAME, sealed))
                .path("/")
                .http_only(true)
                .secure(true)
                // The provider sends the browser back with a top-level GET, which `Lax` admits.
                .same_site(SameSite::Lax)
                .max_age(topcoat::cookie::time::Duration::seconds(
                    i64::try_from(PENDING_LIFETIME.as_secs()).unwrap_or(i64::MAX),
                ))
                .build(),
        );
        private(see_other(url).into_response(cx)?)
    })
}

/// `GET {prefix}/sign-in/{key}/callback` — check the provider's answer against the pending
/// sign-in, trade its code for the identity, and sign the user it links in.
pub(super) fn callback(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current_realm(cx).ok_or_else(not_found)?;
        let oidc = panel.auth.oidc().ok_or_else(not_found)?;
        let (provider, true) = route_of(cx, uri(cx).path()).ok_or_else(not_found)? else {
            return Err(not_found().into());
        };
        // One sign-in, one answer: the cookie goes whatever the answer says.
        let pending = sealed_cookies(cx)?
            .get(COOKIE_NAME)
            .and_then(|cookie| serde_json::from_str::<Pending>(cookie.value()).ok());
        cookies(cx).remove(Cookie::build((COOKIE_NAME, "")).path("/").secure(true));
        let query = query(cx);
        let read = |key: &str| query.iter().find(|(name, _)| name == key).map(|(_, v)| v);
        let now = jiff::Timestamp::now().as_second();
        let lifetime = i64::try_from(PENDING_LIFETIME.as_secs()).unwrap_or(i64::MAX);
        let Some(pending) = pending.filter(|pending| {
            (0..lifetime).contains(&(now - pending.started))
                && pending.panel == panel.prefix
                && pending.provider == provider.key
                && read("state").is_some_and(|state| {
                    subtle::ConstantTimeEq::ct_eq(state.as_bytes(), pending.state.as_bytes()).into()
                })
        }) else {
            return refused(cx, provider, Failure::Refused, "");
        };
        let next = pending.next.clone();
        let Some(code) = read("code").filter(|code| !code.is_empty()) else {
            // The provider's `error`, such as `access_denied` when the user cancels.
            return refused(cx, provider, Failure::Refused, &next);
        };
        // The cookie stays good for its lifetime, so the throttle bounds the token requests one
        // client sends the provider by replaying it.
        let throttle = panel.auth.login_throttle();
        let client = client_ip(cx);
        if !throttle
            .attempt(cx, THROTTLE_SCOPE, &provider.key, client)
            .await?
        {
            tracing::warn!(panel = %panel.prefix, client = ?client, "provider sign-in throttled");
            return refused(cx, provider, Failure::Refused, &next);
        }
        let redirect_uri = oidc.redirect_uri(&panel.prefix, &provider.key);
        let identity = match provider.redeem(code, &redirect_uri, &pending).await {
            Ok(identity) => identity,
            Err(failure) => return refused(cx, provider, failure, &next),
        };
        throttle
            .clear(cx, THROTTLE_SCOPE, &provider.key, client)
            .await?;
        let user = match oidc.linker.sign_in(cx, &identity).await {
            Ok(user) => user,
            Err(error) => {
                let error = infrastructure_failure(error);
                if crate::error::TabloError::is_infrastructure(&error) {
                    return refused(cx, provider, Failure::Unavailable, &next);
                }
                return Err(error);
            }
        };
        let Some(user) = user.filter(|user| user.can_access_panel(&panel.id)) else {
            return refused(cx, provider, Failure::Refused, &next);
        };
        admit(cx, panel, &*user, next).await
    })
}

/// The request's query pairs.
fn query(cx: &Cx) -> Vec<(String, String)> {
    uri(cx)
        .query()
        .map(|query| {
            form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_default()
}

/// The request's cookies sealed under the router's cookie `Key`, which mounting requires.
fn sealed_cookies(
    cx: &Cx,
) -> topcoat::Result<topcoat::cookie::PrivateJar<'_, &topcoat::cookie::CookieJar>> {
    try_private_cookies(cx).ok_or_else(|| {
        crate::error::declaration("provider sign-in needs the router's cookie `Key`".to_owned())
    })
}

/// Back to the login page, carrying `next`, saying the sign-in through `provider` failed.
fn refused(
    cx: &Cx,
    provider: &OidcProvider,
    failure: Failure,
    next: &str,
) -> topcoat::Result<Response> {
    let message = match failure {
        Failure::Refused => "tablo-oidc-failed",
        Failure::Unavailable => "tablo-oidc-unavailable",
    };
    set_notification(
        cx,
        Notification::error(crate::t!(
            cx,
            message,
            provider = provider.label.localize(cx)
        )),
    );
    private(see_other(with_next(login_url(cx), next)).into_response(cx)?)
}

/// Mounts `realm`'s provider routes: a start and a callback per provider.
pub(super) fn routes(
    mut builder: topcoat::router::RouterBuilder,
    realm: &Realm,
) -> topcoat::router::RouterBuilder {
    use topcoat::router::RouteFn;

    let Some(oidc) = realm.auth.oidc() else {
        return builder;
    };
    for provider in &oidc.providers {
        let path = format!("{}/sign-in/{}", realm.prefix, provider.key);
        builder = builder
            .route(RouteFn::new(
                http::Method::GET,
                crate::naming::route_path(&path),
                start,
            ))
            .route(RouteFn::new(
                http::Method::GET,
                crate::naming::route_path(&format!("{path}/callback")),
                callback,
            ));
    }
    builder
}

/// The mistakes in a panel's [`Auth::oidc`]: provider sign-in without auth or beside external
/// auth, a router without the cookie key, a base URL that is not an absolute `https` origin, no
/// provider, a provider key that cannot name a route or names two providers, and an issuer the
/// panel may not send requests to.
pub(super) fn declaration_errors(
    auth: &super::AuthConfig,
    has_key: bool,
) -> Vec<crate::DeclarationErrorKind> {
    use crate::{DeclarationErrorKind, declaration::OidcFault};

    let Some(oidc) = &auth.oidc else {
        return Vec::new();
    };
    if auth.authenticator().is_none() {
        return vec![DeclarationErrorKind::Oidc(OidcFault::AuthDisabled)];
    }
    let mut faults = Vec::new();
    if auth.external().is_some() {
        faults.push(OidcFault::ExternalAuth);
    }
    if !has_key {
        faults.push(OidcFault::MissingKey);
    }
    let origin = url::Url::parse(&oidc.base_url)
        .ok()
        .filter(|url| url.path() == "/" && url.query().is_none() && url.fragment().is_none());
    if !origin.is_some_and(|_| provider::secure_url(&oidc.base_url)) {
        faults.push(OidcFault::BaseUrl(oidc.base_url.clone()));
    }
    if oidc.providers.is_empty() {
        faults.push(OidcFault::NoProvider);
    }
    for (index, provider) in oidc.providers.iter().enumerate() {
        if !provider.key_is_valid() {
            faults.push(OidcFault::ProviderKey(provider.key.clone()));
        } else if oidc.providers[..index]
            .iter()
            .any(|earlier| earlier.key == provider.key)
        {
            faults.push(OidcFault::DuplicateProvider(provider.key.clone()));
        }
        if !provider.issuer_is_valid() {
            faults.push(OidcFault::Issuer(provider.key.clone()));
        }
        for (name, _) in &provider.params {
            if provider::RESERVED_PARAMS.contains(&name.as_str()) {
                faults.push(OidcFault::ReservedParam(provider.key.clone(), name.clone()));
            }
        }
    }
    faults.into_iter().map(DeclarationErrorKind::Oidc).collect()
}
