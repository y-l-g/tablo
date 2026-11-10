use data_encoding::BASE64URL_NOPAD;
use serde_json::{Value, json};

use super::{
    OidcProvider,
    provider::{identity, secure_url},
};

const NOW: i64 = 1_800_000_000;
const NONCE: &str = "the-nonce";

fn provider() -> OidcProvider {
    OidcProvider::new("idp", "IdP", "https://idp.test", "client", "secret")
}

/// Claims every check accepts.
fn claims() -> Value {
    json!({
        "iss": "https://idp.test",
        "sub": "account-1",
        "aud": "client",
        "exp": NOW + 600,
        "iat": NOW,
        "nonce": NONCE,
        "email": "ada@example.com",
        "email_verified": true,
        "name": "Ada",
    })
}

fn token(claims: &Value) -> String {
    let header = BASE64URL_NOPAD.encode(br#"{"alg":"RS256"}"#);
    let payload = BASE64URL_NOPAD.encode(claims.to_string().as_bytes());
    format!("{header}.{payload}.signature")
}

fn with(change: impl FnOnce(&mut serde_json::Map<String, Value>)) -> Value {
    let mut claims = claims();
    change(claims.as_object_mut().expect("the claims are an object"));
    claims
}

#[test]
fn a_token_passing_every_check_names_its_account() {
    let identity = identity(&provider(), &token(&claims()), NONCE, NOW).expect("accepted");
    assert_eq!(
        (
            identity.provider.as_str(),
            identity.issuer.as_str(),
            identity.subject.as_str(),
            identity.email.as_deref(),
            identity.email_verified,
            identity.name.as_deref(),
        ),
        (
            "idp",
            "https://idp.test",
            "account-1",
            Some("ada@example.com"),
            true,
            Some("Ada")
        )
    );
}

/// Each check refuses alone: another issuer, another client, a token for several clients that
/// does not name this one as its party, an expired token, one issued in the future, another
/// sign-in's nonce, no nonce, no subject, and a token that is not three parts.
#[test]
fn a_token_failing_one_check_names_nobody() {
    let refused = [
        with(|c| drop(c.insert("iss".into(), json!("https://other.test")))),
        with(|c| drop(c.insert("aud".into(), json!("another-client")))),
        with(|c| drop(c.insert("aud".into(), json!(["client", "another-client"])))),
        with(|c| {
            c.insert("aud".into(), json!(["client", "another-client"]));
            c.insert("azp".into(), json!("another-client"));
        }),
        with(|c| drop(c.insert("exp".into(), json!(NOW - 3600)))),
        with(|c| drop(c.insert("iat".into(), json!(NOW + 3600)))),
        with(|c| drop(c.insert("nonce".into(), json!("another-nonce")))),
        with(|c| drop(c.remove("nonce"))),
        with(|c| drop(c.insert("sub".into(), json!("")))),
    ];
    for claims in refused {
        assert_eq!(
            identity(&provider(), &token(&claims), NONCE, NOW),
            None,
            "{claims}"
        );
    }
    let two_parts = token(&claims()).rsplit_once('.').unwrap().0.to_string();
    assert_eq!(identity(&provider(), &two_parts, NONCE, NOW), None);
}

#[test]
fn a_token_for_several_clients_names_this_one_as_its_party() {
    let claims = with(|c| {
        c.insert("aud".into(), json!(["client", "another-client"]));
        c.insert("azp".into(), json!("client"));
    });
    assert!(identity(&provider(), &token(&claims), NONCE, NOW).is_some());
}

/// Only `true`, or the string some providers send for it, verifies an address.
#[test]
fn an_address_is_verified_only_when_the_provider_says_so() {
    let verified = |flag: Option<Value>| {
        let claims = with(|c| match flag {
            Some(flag) => drop(c.insert("email_verified".into(), flag)),
            None => drop(c.remove("email_verified")),
        });
        identity(&provider(), &token(&claims), NONCE, NOW)
            .expect("accepted")
            .email_verified
    };
    assert!(verified(Some(json!(true))));
    assert!(verified(Some(json!("true"))));
    assert!(!verified(Some(json!(false))));
    assert!(!verified(Some(json!("false"))));
    assert!(!verified(None));
}

/// Google's tokens carry its issuer with or without the scheme; both name the same issuer.
#[test]
fn google_accepts_both_spellings_of_its_issuer() {
    let google = OidcProvider::google("client", "secret");
    for iss in ["https://accounts.google.com", "accounts.google.com"] {
        let claims = with(|c| drop(c.insert("iss".into(), json!(iss))));
        let identity = identity(&google, &token(&claims), NONCE, NOW).expect("accepted");
        assert_eq!(identity.issuer, "https://accounts.google.com");
    }
}

#[test]
fn only_https_or_loopback_http_is_secure() {
    for url in [
        "https://idp.test",
        "http://127.0.0.1:8080",
        "http://localhost:3000/realms/x",
        "http://[::1]:9000",
    ] {
        assert!(secure_url(url), "{url}");
    }
    for url in [
        "http://idp.test",
        "http://127.0.0.1.idp.test",
        "ftp://idp.test",
        "idp.test",
    ] {
        assert!(!secure_url(url), "{url}");
    }
}

/// An issuer is matched as the provider spells it: Auth0's ends in `/`, and so do its tokens.
#[test]
fn an_issuer_ending_in_a_slash_keeps_it() {
    let auth0 = OidcProvider::new(
        "auth0",
        "Auth0",
        "https://tenant.auth0.test/",
        "client",
        "secret",
    );
    let slashed = with(|c| drop(c.insert("iss".into(), json!("https://tenant.auth0.test/"))));
    let identity = identity(&auth0, &token(&slashed), NONCE, NOW).expect("accepted");
    assert_eq!(identity.issuer, "https://tenant.auth0.test/");
    let bare = with(|c| drop(c.insert("iss".into(), json!("https://tenant.auth0.test"))));
    assert_eq!(identity_of(&auth0, &bare), None);
}

fn identity_of(provider: &OidcProvider, claims: &Value) -> Option<super::ProviderIdentity> {
    identity(provider, &token(claims), NONCE, NOW)
}

/// Every claim reaches the linker, such as Google's `hd`.
#[test]
fn the_identity_carries_every_claim() {
    let claims = with(|c| drop(c.insert("hd".into(), json!("example.com"))));
    let identity = identity_of(&provider(), &claims).expect("accepted");
    assert_eq!(identity.claims.get("hd"), Some(&json!("example.com")));
}

#[tokio::test]
async fn the_authorization_url_carries_extra_scopes_and_params() {
    let google = OidcProvider::google("client", "secret")
        .scope("https://www.googleapis.com/auth/calendar.readonly")
        .param("prompt", "select_account");
    let pending = super::Pending::new("/admin".into(), "google".into(), String::new());
    let url = google
        .authorization_url("https://app.test/admin/sign-in/google/callback", &pending)
        .await
        .expect("Google's endpoints are fixed");
    let query: std::collections::HashMap<String, String> =
        form_urlencoded::parse(url.split_once('?').expect("a query").1.as_bytes())
            .into_owned()
            .collect();
    assert_eq!(
        query["scope"],
        "openid email profile https://www.googleapis.com/auth/calendar.readonly"
    );
    assert_eq!(query["prompt"], "select_account");
    assert_eq!(query["state"], pending.state);
}
