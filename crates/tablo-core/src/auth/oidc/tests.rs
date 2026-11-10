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
