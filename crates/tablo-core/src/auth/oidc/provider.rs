//! [`OidcProvider`]: one OpenID Connect provider, the requests a panel sends it, and the checks
//! its ID token passes before the panel reads who it names.

use std::{sync::Arc, time::Duration};

use data_encoding::{BASE64, BASE64URL_NOPAD};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::ProviderIdentity;
use crate::Text;

/// How long a request to a provider may take before the sign-in reads as unavailable.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// The most bytes a provider's answer may hold: a discovery document or a token response is a
/// few kilobytes.
const MAX_RESPONSE_BYTES: u64 = 256 * 1024;

/// How far a provider's clock may run from the panel's when an ID token says when it was
/// issued and when it expires.
const CLOCK_SKEW_SECS: i64 = 300;

/// What the panel asks every provider for: an ID token, the account's email and its name.
const SCOPES: &str = "openid email profile";

/// One OpenID Connect provider a panel signs users in through, installed with
/// [`Oidc::provider`](super::Oidc::provider).
///
/// [`OidcProvider::google`] is Google. [`OidcProvider::new`] is any other provider that publishes
/// a discovery document at `{issuer}/.well-known/openid-configuration` and returns its ID token
/// from the token endpoint: the panel reads the endpoints from that document on the first sign-in
/// and keeps them.
///
/// The panel registers `{base_url}{prefix}/sign-in/{key}/callback` as its redirect URI, where
/// `base_url` is [`Oidc::new`](super::Oidc::new)'s; register the same URI with the provider.
pub struct OidcProvider {
    pub(super) key: String,
    pub(super) label: Text,
    issuer: String,
    /// Every `iss` an ID token of this provider may carry: the issuer, and for Google its
    /// scheme-less spelling.
    issuers: Vec<String>,
    client_id: String,
    client_secret: String,
    endpoints: Arc<tokio::sync::OnceCell<Endpoints>>,
}

impl OidcProvider {
    /// Google, signing in the client `client_id` with `client_secret` from a Google Cloud
    /// project's OAuth client of the "Web application" type. Its key is `google`, so its redirect
    /// URI is `{base_url}{prefix}/sign-in/google/callback`.
    #[must_use]
    pub fn google(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        let provider = Self::new(
            "google",
            "Google",
            "https://accounts.google.com",
            client_id,
            client_secret,
        );
        // Google's endpoints are fixed, so a sign-in skips the discovery request.
        let endpoints = Endpoints {
            authorization: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token: "https://oauth2.googleapis.com/token".to_string(),
        };
        Self {
            // Google's own documentation says its ID tokens carry either spelling.
            issuers: vec![
                "https://accounts.google.com".to_string(),
                "accounts.google.com".to_string(),
            ],
            endpoints: Arc::new(tokio::sync::OnceCell::new_with(Some(endpoints))),
            ..provider
        }
    }

    /// The provider whose issuer is `issuer`, such as `https://login.example.com`, signing in the
    /// client `client_id` with `client_secret`. `key` names it in its routes, such as `okta`:
    /// lowercase ASCII letters, digits and `-`. `label` is the name its button shows.
    ///
    /// `issuer` is an `https` URL, or an `http` one on a loopback host for a provider running on
    /// the same machine; anything else is a declaration error.
    #[must_use]
    pub fn new(
        key: impl Into<String>,
        label: impl Into<Text>,
        issuer: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        let issuer = issuer.into().trim_end_matches('/').to_string();
        Self {
            key: key.into(),
            label: label.into(),
            issuers: vec![issuer.clone()],
            issuer,
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            endpoints: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    /// Whether the key may name a route segment.
    pub(super) fn key_is_valid(&self) -> bool {
        !self.key.is_empty()
            && self
                .key
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    }

    /// Whether the issuer is a URL the panel may send requests to.
    pub(super) fn issuer_is_valid(&self) -> bool {
        secure_url(&self.issuer)
    }

    /// The URL the browser leaves for to sign in, asking for a code sent back to `redirect_uri`.
    pub(super) async fn authorization_url(
        &self,
        redirect_uri: &str,
        pending: &Pending,
    ) -> Result<String, Failure> {
        let endpoints = self.endpoints().await?;
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", SCOPES)
            .append_pair("state", &pending.state)
            .append_pair("nonce", &pending.nonce)
            .append_pair("code_challenge", &code_challenge(&pending.verifier))
            .append_pair("code_challenge_method", "S256")
            .finish();
        let separator = if endpoints.authorization.contains('?') {
            '&'
        } else {
            '?'
        };
        Ok(format!("{}{separator}{query}", endpoints.authorization))
    }

    /// The identity `code` signs in: the code traded at the token endpoint for an ID token, which
    /// [`identity`] checks against `pending`.
    pub(super) async fn redeem(
        &self,
        code: &str,
        redirect_uri: &str,
        pending: &Pending,
    ) -> Result<ProviderIdentity, Failure> {
        let endpoints = self.endpoints().await?;
        let form = [
            ("grant_type".to_string(), "authorization_code".to_string()),
            ("code".to_string(), code.to_string()),
            ("redirect_uri".to_string(), redirect_uri.to_string()),
            ("code_verifier".to_string(), pending.verifier.clone()),
        ];
        // RFC 6749 §2.3.1: the client's id and secret are form-encoded before they are joined.
        let encode =
            |value: &str| form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>();
        let credentials = format!(
            "{}:{}",
            encode(&self.client_id),
            encode(&self.client_secret)
        );
        let authorization = format!("Basic {}", BASE64.encode(credentials.as_bytes()));
        let token = endpoints.token.clone();
        let answer: TokenResponse = request(move |agent| {
            agent
                .post(&token)
                .header("authorization", &authorization)
                .header("accept", "application/json")
                .send_form(
                    form.iter()
                        .map(|(key, value)| (key.as_str(), value.as_str())),
                )
        })
        .await?;
        let now = jiff::Timestamp::now().as_second();
        identity(self, &answer.id_token, &pending.nonce, now).ok_or(Failure::Refused)
    }

    /// The provider's endpoints, read from its discovery document on first use. A failed read is
    /// not kept, so the next sign-in tries again.
    async fn endpoints(&self) -> Result<&Endpoints, Failure> {
        self.endpoints
            .get_or_try_init(|| discover(self.issuer.clone()))
            .await
    }
}

/// Redacts the client secret, so a provider logged with `?` writes none to the log.
impl std::fmt::Debug for OidcProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcProvider")
            .field("key", &self.key)
            .field("issuer", &self.issuer)
            .field("client_id", &self.client_id)
            .field("client_secret", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// Where a provider signs a browser in and trades a code for tokens.
#[derive(Debug)]
struct Endpoints {
    authorization: String,
    token: String,
}

/// What a sign-in remembers between leaving for the provider and coming back: the secrets the
/// answer must match.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub(super) struct Pending {
    /// The panel the sign-in started on.
    pub(super) panel: String,
    /// The provider's key.
    pub(super) provider: String,
    /// Returned by the provider with the code: binds the answer to this browser.
    pub(super) state: String,
    /// Carried in the ID token: binds the token to this sign-in.
    pub(super) nonce: String,
    /// The PKCE verifier the token request proves the code was asked for here with.
    pub(super) verifier: String,
    /// Where to land once signed in, already a same-origin path.
    pub(super) next: String,
}

impl Pending {
    /// A sign-in on `panel` through `provider`, with fresh secrets.
    pub(super) fn new(panel: String, provider: String, next: String) -> Self {
        Self {
            panel,
            provider,
            state: secret(),
            nonce: secret(),
            verifier: secret(),
            next,
        }
    }
}

/// Why a provider did not sign the browser in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    /// The provider could not be reached, or answered with a server error.
    Unavailable,
    /// The provider refused, or its answer failed a check.
    Refused,
}

/// 256 random bits in URL-safe base64: a state, a nonce or a PKCE verifier.
fn secret() -> String {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).expect("the sign-in randomness source failed");
    BASE64URL_NOPAD.encode(&bytes)
}

/// The PKCE `S256` challenge of `verifier` (RFC 7636 §4.2).
pub(super) fn code_challenge(verifier: &str) -> String {
    BASE64URL_NOPAD.encode(&Sha256::digest(verifier.as_bytes()))
}

/// Whether the panel may send a request to `url`: `https`, or `http` on a loopback host.
pub(super) fn secure_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    match parsed.scheme() {
        "https" => parsed.host().is_some(),
        "http" => match parsed.host() {
            Some(url::Host::Domain(domain)) => domain == "localhost",
            Some(url::Host::Ipv4(address)) => address.is_loopback(),
            Some(url::Host::Ipv6(address)) => address.is_loopback(),
            None => false,
        },
        _ => false,
    }
}

/// Reads the endpoints of the provider at `issuer` from its discovery document, which must name
/// the same issuer.
async fn discover(issuer: String) -> Result<Endpoints, Failure> {
    #[derive(Deserialize)]
    struct Discovery {
        issuer: String,
        authorization_endpoint: String,
        token_endpoint: String,
    }
    let url = format!("{issuer}/.well-known/openid-configuration");
    let document: Discovery = request(move |agent| agent.get(&url).call()).await?;
    // OpenID Connect Discovery §4.3: a document naming another issuer is not this provider's.
    let endpoints_secure =
        secure_url(&document.authorization_endpoint) && secure_url(&document.token_endpoint);
    if document.issuer.trim_end_matches('/') != issuer || !endpoints_secure {
        tracing::error!(%issuer, "an OpenID Connect discovery document failed its checks");
        return Err(Failure::Refused);
    }
    Ok(Endpoints {
        authorization: document.authorization_endpoint,
        token: document.token_endpoint,
    })
}

/// The token endpoint's answer, of which the panel reads the ID token alone.
#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
}

/// Sends the request `send` builds on the blocking pool and reads its JSON answer. A transport
/// failure or a server error is [`Failure::Unavailable`]; any other answer but a success, or a
/// body that does not parse, is [`Failure::Refused`].
async fn request<T, F>(send: F) -> Result<T, Failure>
where
    T: serde::de::DeserializeOwned + Send + 'static,
    F: FnOnce(&ureq::Agent) -> Result<http::Response<ureq::Body>, ureq::Error> + Send + 'static,
{
    let answer = tokio::task::spawn_blocking(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(REQUEST_TIMEOUT))
            .http_status_as_error(false)
            .max_redirects(0)
            .build()
            .into();
        let mut response = send(&agent).map_err(|error| {
            tracing::warn!(%error, "an OpenID Connect provider could not be reached");
            Failure::Unavailable
        })?;
        let status = response.status();
        if status.is_server_error() {
            tracing::warn!(%status, "an OpenID Connect provider answered with a server error");
            return Err(Failure::Unavailable);
        }
        if !status.is_success() {
            tracing::warn!(%status, "an OpenID Connect provider refused a request");
            return Err(Failure::Refused);
        }
        response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_string()
            .map_err(|_| Failure::Unavailable)
    })
    .await
    .map_err(|_| Failure::Unavailable)??;
    serde_json::from_str(&answer).map_err(|error| {
        tracing::warn!(%error, "an OpenID Connect provider's answer does not parse");
        Failure::Refused
    })
}

/// The identity `id_token` names, when its claims pass OpenID Connect Core §3.1.3.7 for
/// `provider` at `now`, in seconds since the epoch, and carry `nonce`; `None` otherwise.
///
/// The signature is not checked: the token came straight from the token endpoint over TLS, which
/// §3.1.3.7 accepts in its place.
pub(super) fn identity(
    provider: &OidcProvider,
    id_token: &str,
    nonce: &str,
    now: i64,
) -> Option<ProviderIdentity> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Audience {
        One(String),
        Many(Vec<String>),
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Flag {
        Bool(bool),
        // Some providers send booleans as strings.
        Text(String),
    }
    #[derive(Deserialize)]
    struct Claims {
        iss: String,
        sub: String,
        aud: Audience,
        azp: Option<String>,
        exp: i64,
        iat: i64,
        nonce: Option<String>,
        email: Option<String>,
        email_verified: Option<Flag>,
        name: Option<String>,
    }
    let mut parts = id_token.split('.');
    let (Some(_header), Some(payload), Some(_signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let payload = BASE64URL_NOPAD
        .decode(payload.trim_end_matches('=').as_bytes())
        .ok()?;
    let claims: Claims = serde_json::from_slice(&payload).ok()?;
    let audience = match claims.aud {
        Audience::One(one) => vec![one],
        Audience::Many(many) => many,
    };
    let issued_here = provider.issuers.contains(&claims.iss);
    let addressed_here = audience.contains(&provider.client_id)
        && (audience.len() == 1 || claims.azp.is_some())
        && claims
            .azp
            .as_ref()
            .is_none_or(|azp| *azp == provider.client_id);
    let live = claims.exp > now - CLOCK_SKEW_SECS && claims.iat < now + CLOCK_SKEW_SECS;
    let ours = claims.nonce.as_deref().is_some_and(|claimed| {
        subtle::ConstantTimeEq::ct_eq(claimed.as_bytes(), nonce.as_bytes()).into()
    });
    if !(issued_here && addressed_here && live && ours) || claims.sub.is_empty() {
        return None;
    }
    let email_verified = match claims.email_verified {
        Some(Flag::Bool(verified)) => verified,
        Some(Flag::Text(text)) => text == "true",
        None => false,
    };
    Some(ProviderIdentity {
        provider: provider.key.clone(),
        issuer: provider.issuer.clone(),
        subject: claims.sub,
        email: claims.email.filter(|email| !email.is_empty()),
        email_verified,
        name: claims.name.filter(|name| !name.is_empty()),
    })
}
