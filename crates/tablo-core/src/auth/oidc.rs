//! Provider sign-in: the [`IdentityLinker`] trait that turns a provider's identity into the app's
//! user, the [`Oidc`] configuration, and the `{prefix}/sign-in/{key}` routes that leave for an
//! OpenID Connect provider and come back from it.

mod provider;

use std::{future::Future, marker::PhantomData, sync::Arc, time::Duration};

use topcoat::{
    context::Cx,
    cookie::{Cookie, Cookies, SameSite, cookies, try_private_cookies},
    router::{
        Body, RouteFuture,
        error::{not_found, see_other},
        request::uri,
        response::{IntoResponse, Response},
    },
    view::{BoxView, ViewExt, view},
};

pub use self::provider::OidcProvider;
use self::provider::{Failure, Pending};
use super::{
    Auth, PanelUser, infrastructure_failure,
    login::{admit, login_url, next_from_query},
    mail::private,
    realm::{Realm, current_realm, panel_prefix},
    registration::with_next,
};
use crate::{BoxFuture, flash::set_notification, outcome::Notification};

#[cfg(test)]
mod tests;

/// How long a browser has to come back from the provider.
const PENDING_LIFETIME: Duration = Duration::from_mins(10);

/// The cookie a pending sign-in travels in, sealed under the router's cookie `Key`.
const COOKIE_NAME: &str = "__Host-tablo-oidc";

/// Who an OpenID Connect provider says signed in, once the panel has checked its ID token: the
/// token came from the provider's token endpoint, names the panel's client, has not expired and
/// carries the nonce this sign-in sent.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProviderIdentity {
    /// The provider's key, such as `google`.
    pub provider: String,
    /// The provider's issuer, such as `https://accounts.google.com`.
    pub issuer: String,
    /// The provider's id for the account (`sub`): with the issuer, the one stable name of the
    /// account. An email address can change hands; this cannot.
    pub subject: String,
    /// The account's email address, as the provider has it.
    pub email: Option<String>,
    /// Whether the provider says the account proved it reads `email`.
    pub email_verified: bool,
    /// The account's display name.
    pub name: Option<String>,
}

/// How a panel turns a [`ProviderIdentity`] into one of the app's users, installed with
/// [`Auth::oidc`] through an [`Oidc`].
///
/// The app stores which of its users each identity is, as a row keyed by
/// [`issuer`](ProviderIdentity::issuer) and [`subject`](ProviderIdentity::subject). A sign-in
/// whose identity no row names may link it to an existing account, or create one, as the app
/// decides; linking by email alone hands that account to whoever controls the address at the
/// provider, so link only an address the provider [verified](ProviderIdentity::email_verified).
///
/// ```rust
/// # use tablo_core::{IdentityLinker, PanelUser, ProviderIdentity};
/// # use topcoat::context::Cx;
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Staff {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[unique] email: String,
/// #     name: String,
/// # }
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { self.id.to_string() }
/// #     fn display_name(&self) -> &str { &self.name }
/// #     fn can_access_panel(&self, _panel: &str) -> bool { true }
/// # }
/// #[derive(Debug, Clone, toasty::Model)]
/// struct StaffIdentity {
///     #[key]
///     #[auto]
///     id: uuid::Uuid,
///     #[index]
///     staff_id: uuid::Uuid,
///     issuer: String,
///     subject: String,
/// }
///
/// struct StaffLinker;
///
/// impl IdentityLinker for StaffLinker {
///     type User = Staff;
///
///     async fn sign_in(
///         &self,
///         _cx: &Cx,
///         identity: &ProviderIdentity,
///         ex: &mut dyn toasty::Executor,
///     ) -> topcoat::Result<Option<Staff>> {
///         let linked = StaffIdentity::filter(
///             StaffIdentity::fields()
///                 .issuer()
///                 .eq(identity.issuer.clone())
///                 .and(
///                     StaffIdentity::fields()
///                         .subject()
///                         .eq(identity.subject.clone()),
///                 ),
///         )
///         .first()
///         .exec(&mut *ex)
///         .await?;
///         if let Some(linked) = linked {
///             return Ok(Staff::filter_by_id(linked.staff_id)
///                 .first()
///                 .exec(ex)
///                 .await?);
///         }
///         // Link an existing account only by an address the provider verified.
///         let (Some(email), true) = (&identity.email, identity.email_verified) else {
///             return Ok(None);
///         };
///         let Some(staff) = Staff::filter_by_email(email).first().exec(&mut *ex).await? else {
///             return Ok(None);
///         };
///         toasty::create!(StaffIdentity {
///             staff_id: staff.id,
///             issuer: identity.issuer.clone(),
///             subject: identity.subject.clone(),
///         })
///         .exec(ex)
///         .await?;
///         Ok(Some(staff))
///     }
/// }
/// ```
pub trait IdentityLinker: Send + Sync + 'static {
    /// The account an identity signs in as: the type the panel's
    /// [`Authenticator`](super::Authenticator) loads, which `Auth<U>` checks.
    type User: PanelUser;

    /// The user `identity` signs in as, linking or creating one as the app decides, through the
    /// sign-in's transaction `ex`, which commits when it returns `Ok`. `None` refuses the sign-in,
    /// which reads as a failed one. A user who may not
    /// [access the panel](PanelUser::can_access_panel) is refused too.
    fn sign_in(
        &self,
        cx: &Cx,
        identity: &ProviderIdentity,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<Option<Self::User>>> + Send;
}

/// [`IdentityLinker`] with its user type erased, as the panel keeps it.
trait DynLinker: Send + Sync {
    fn sign_in<'a>(
        &'a self,
        cx: &'a Cx,
        identity: &'a ProviderIdentity,
    ) -> BoxFuture<'a, topcoat::Result<Option<Arc<dyn PanelUser>>>>;
}

impl<L: IdentityLinker> DynLinker for L {
    fn sign_in<'a>(
        &'a self,
        cx: &'a Cx,
        identity: &'a ProviderIdentity,
    ) -> BoxFuture<'a, topcoat::Result<Option<Arc<dyn PanelUser>>>> {
        Box::pin(async move {
            let mut db = crate::db::db(cx)?;
            let mut tx = db.transaction().await.map_err(infrastructure_failure)?;
            let user = IdentityLinker::sign_in(self, cx, identity, &mut tx).await?;
            tx.commit().await.map_err(infrastructure_failure)?;
            Ok(user.map(|user| Arc::new(user) as Arc<dyn PanelUser>))
        })
    }
}

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
        let Some(pending) = pending.filter(|pending| {
            pending.panel == panel.prefix
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
        let redirect_uri = oidc.redirect_uri(&panel.prefix, &provider.key);
        let identity = match provider.redeem(code, &redirect_uri, &pending).await {
            Ok(identity) => identity,
            Err(failure) => return refused(cx, provider, failure, &next),
        };
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
        let Some(user) = user.filter(|user| user.can_access_panel(&panel.prefix)) else {
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
    }
    faults.into_iter().map(DeclarationErrorKind::Oidc).collect()
}
