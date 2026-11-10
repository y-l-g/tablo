//! [`FakeProvider`]: an OpenID Connect provider on a loopback port, for a test of a panel's
//! provider sign-in to sign accounts in through without the network.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use data_encoding::{BASE64, BASE64URL_NOPAD};
use sha2::{Digest, Sha256};
use tablo_core::OidcProvider;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

/// An account at a [`FakeProvider`]: what the ID token it issues says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeAccount {
    /// The provider's id for the account (`sub`).
    pub subject: String,
    /// The account's address.
    pub email: Option<String>,
    /// Whether the provider says the account proved it reads the address.
    pub email_verified: bool,
    /// The account's display name.
    pub name: Option<String>,
}

impl FakeAccount {
    /// The account `subject` holding the verified address `email`.
    #[must_use]
    pub fn verified(subject: &str, email: &str) -> Self {
        Self {
            subject: subject.to_string(),
            email: Some(email.to_string()),
            email_verified: true,
            name: None,
        }
    }
}

/// A code the fake handed out, waiting for the token request that redeems it.
struct Grant {
    account: FakeAccount,
    nonce: String,
    challenge: String,
    redirect_uri: String,
}

#[derive(Default)]
struct State {
    grants: HashMap<String, Grant>,
    redeemed: usize,
}

/// An OpenID Connect provider serving its discovery document and token endpoint on a loopback
/// port, which a panel's [`OidcProvider`] reaches as it would a real one.
///
/// The browser's visit to the provider is [`authorize`](Self::authorize): it reads the
/// authorization URL the panel redirected to and answers the callback the provider would send
/// the browser back to. The token endpoint checks the client's secret, the PKCE verifier and the
/// redirect URI, takes each code once, and issues an ID token carrying the nonce the panel sent.
pub struct FakeProvider {
    issuer: String,
    client_id: String,
    client_secret: String,
    state: Arc<Mutex<State>>,
    server: tokio::task::JoinHandle<()>,
}

impl FakeProvider {
    /// A provider for the client `client_id` with `client_secret`, listening on a free loopback
    /// port until dropped.
    ///
    /// # Panics
    ///
    /// When no loopback port is free.
    pub async fn start(client_id: &str, client_secret: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is free");
        let issuer = format!(
            "http://{}",
            listener
                .local_addr()
                .expect("a bound listener has an address")
        );
        let state = Arc::new(Mutex::new(State::default()));
        let server = tokio::spawn(serve(
            listener,
            Served {
                issuer: issuer.clone(),
                client_id: client_id.to_string(),
                client_secret: client_secret.to_string(),
                state: Arc::clone(&state),
            },
        ));
        Self {
            issuer,
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            state,
            server,
        }
    }

    /// The issuer, `http://127.0.0.1:{port}`.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// This provider as a panel installs it, under `key`, labelled `key`.
    #[must_use]
    pub fn provider(&self, key: &str) -> OidcProvider {
        OidcProvider::new(
            key,
            key,
            self.issuer.clone(),
            self.client_id.clone(),
            self.client_secret.clone(),
        )
    }

    /// Signs `account` in at `location`, the authorization URL a panel redirected the browser
    /// to, and answers the path and query the provider sends the browser back to.
    ///
    /// # Panics
    ///
    /// When `location` is not this provider's authorization URL for its client, or asks for no
    /// PKCE `S256` challenge.
    #[must_use]
    pub fn authorize(&self, location: &str, account: &FakeAccount) -> String {
        let query = self.authorization(location);
        let code = BASE64URL_NOPAD.encode(uuid::Uuid::new_v4().as_bytes());
        let redirect_uri = query["redirect_uri"].clone();
        self.state.lock().expect("the fake's lock").grants.insert(
            code.clone(),
            Grant {
                account: account.clone(),
                nonce: query["nonce"].clone(),
                challenge: query["code_challenge"].clone(),
                redirect_uri: redirect_uri.clone(),
            },
        );
        callback(
            &redirect_uri,
            &[("code", &code), ("state", &query["state"])],
        )
    }

    /// The account declines at `location`: the path and query the provider sends the browser
    /// back to, carrying `error=access_denied`.
    ///
    /// # Panics
    ///
    /// As [`authorize`](Self::authorize).
    #[must_use]
    pub fn deny(&self, location: &str) -> String {
        let query = self.authorization(location);
        callback(
            &query["redirect_uri"],
            &[("error", "access_denied"), ("state", &query["state"])],
        )
    }

    /// How many codes the token endpoint has redeemed.
    ///
    /// # Panics
    ///
    /// When a request to the fake panicked while holding its lock.
    #[must_use]
    pub fn redeemed(&self) -> usize {
        self.state.lock().expect("the fake's lock").redeemed
    }

    /// The query of `location`, checked as this provider's authorization request.
    fn authorization(&self, location: &str) -> HashMap<String, String> {
        let (endpoint, query) = location
            .split_once('?')
            .expect("an authorization URL carries a query");
        assert_eq!(endpoint, format!("{}/authorize", self.issuer));
        let query: HashMap<String, String> = form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(query["client_id"], self.client_id);
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["code_challenge_method"], "S256");
        query
    }
}

impl Drop for FakeProvider {
    fn drop(&mut self) {
        self.server.abort();
    }
}

/// The path and query of `redirect_uri` with `pairs` appended: where the browser lands.
fn callback(redirect_uri: &str, pairs: &[(&str, &str)]) -> String {
    let path = redirect_uri
        .find("://")
        .and_then(|scheme| {
            let rest = &redirect_uri[scheme + 3..];
            rest.find('/').map(|slash| &rest[slash..])
        })
        .expect("a redirect URI is absolute");
    let mut query = form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        query.append_pair(key, value);
    }
    format!("{path}?{}", query.finish())
}

/// What the server task answers with.
struct Served {
    issuer: String,
    client_id: String,
    client_secret: String,
    state: Arc<Mutex<State>>,
}

/// Answers one request per connection until the task is aborted.
async fn serve(listener: TcpListener, served: Served) {
    while let Ok((mut stream, _)) = listener.accept().await {
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        let (head, body) = loop {
            let Ok(read) = stream.read(&mut buffer).await else {
                break (String::new(), Vec::new());
            };
            if read == 0 {
                break (String::new(), Vec::new());
            }
            request.extend_from_slice(&buffer[..read]);
            let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let head = String::from_utf8_lossy(&request[..end]).into_owned();
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if request.len() >= end + 4 + length {
                break (head, request[end + 4..end + 4 + length].to_vec());
            }
        };
        let (status, json) = served.answer(&head, &body);
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
             connection: close\r\n\r\n{json}",
            json.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    }
}

impl Served {
    /// The status line and JSON body answering the request `head` with `body`.
    fn answer(&self, head: &str, body: &[u8]) -> (&'static str, String) {
        let mut request_line = head.lines().next().unwrap_or("").split(' ');
        let (method, path) = (request_line.next(), request_line.next());
        match (method, path) {
            (Some("GET"), Some("/.well-known/openid-configuration")) => (
                "200 OK",
                serde_json::json!({
                    "issuer": self.issuer,
                    "authorization_endpoint": format!("{}/authorize", self.issuer),
                    "token_endpoint": format!("{}/token", self.issuer),
                })
                .to_string(),
            ),
            (Some("POST"), Some("/token")) => match self.token(head, body) {
                Some(id_token) => (
                    "200 OK",
                    serde_json::json!({
                        "access_token": "unused",
                        "token_type": "Bearer",
                        "id_token": id_token,
                    })
                    .to_string(),
                ),
                None => (
                    "400 Bad Request",
                    serde_json::json!({ "error": "invalid_grant" }).to_string(),
                ),
            },
            _ => ("404 Not Found", "{}".to_string()),
        }
    }

    /// The ID token a token request redeems, when its client authenticates, its code is live
    /// and its verifier and redirect URI match the code's.
    fn token(&self, head: &str, body: &[u8]) -> Option<String> {
        let expected = format!(
            "Basic {}",
            BASE64.encode(format!("{}:{}", self.client_id, self.client_secret).as_bytes())
        );
        let authenticated = head.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("authorization") && value.trim() == expected
            })
        });
        let form: HashMap<String, String> = form_urlencoded::parse(body).into_owned().collect();
        if !authenticated || form.get("grant_type")? != "authorization_code" {
            return None;
        }
        let mut state = self.state.lock().ok()?;
        let grant = state.grants.remove(form.get("code")?)?;
        let challenge = BASE64URL_NOPAD.encode(&Sha256::digest(form.get("code_verifier")?));
        if challenge != grant.challenge || *form.get("redirect_uri")? != grant.redirect_uri {
            return None;
        }
        state.redeemed += 1;
        let now = jiff::Timestamp::now().as_second();
        let claims = serde_json::json!({
            "iss": self.issuer,
            "sub": grant.account.subject,
            "aud": self.client_id,
            "exp": now + 3600,
            "iat": now,
            "nonce": grant.nonce,
            "email": grant.account.email,
            "email_verified": grant.account.email_verified,
            "name": grant.account.name,
        });
        let header = BASE64URL_NOPAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = BASE64URL_NOPAD.encode(claims.to_string().as_bytes());
        Some(format!("{header}.{payload}.signature"))
    }
}

/// The provider buttons a login or sign-up page shows, as each provider's key and the link that
/// starts its sign-in, in order.
pub fn provider_buttons(html: &str) -> Vec<(String, String)> {
    use crate::protocol::dom::{attr_value, tags_where};

    tags_where(html, |tag, attrs| {
        tag == "a" && attr_value(attrs, "data-oidc-provider").is_some()
    })
    .into_iter()
    .filter_map(|(_, attrs, _)| {
        Some((
            attr_value(&attrs, "data-oidc-provider")?,
            attr_value(&attrs, "href")?,
        ))
    })
    .collect()
}
