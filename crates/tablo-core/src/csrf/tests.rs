use super::*;
use crate::test_support::cx_with_cookie;

#[test]
fn valid_token_format() {
    assert!(is_valid_token(&uuid::Uuid::new_v4().to_string()));
    assert!(!is_valid_token(""));
    assert!(!is_valid_token("not-a-uuid"));
}

#[test]
fn verify_matches_cookie_and_rejects_mismatch() {
    let token = uuid::Uuid::new_v4().to_string();
    let cx = cx_with_cookie(COOKIE_NAME, Some(&token));
    let mut values = std::collections::HashMap::new();
    values.insert(FIELD_NAME.to_string(), token.clone());
    assert!(verify(&cx, &values).is_ok());
    values.insert(FIELD_NAME.to_string(), uuid::Uuid::new_v4().to_string());
    assert!(verify(&cx, &values).is_err());
    assert!(verify(&cx, &std::collections::HashMap::new()).is_err());
    assert!(
        verify(
            &cx_with_cookie(COOKIE_NAME, None),
            &std::collections::HashMap::from([(FIELD_NAME.to_string(), token)])
        )
        .is_err()
    );
}

/// The issued cookie carries the hardened `__Host-` contract,
/// matching the session cookie: `Secure`, `HttpOnly`, `SameSite=Lax`,
/// `Path=/`, no `Domain`.
#[test]
fn ensured_cookie_is_host_prefixed_and_secure() {
    let cx = cx_with_cookie(COOKIE_NAME, None);
    let token = ensure_token(&cx);
    assert!(is_valid_token(&token));
    let cookie = cookies(&cx)
        .get(COOKIE_NAME)
        .expect("ensure_token set the cookie");
    assert_eq!(cookie.value(), token);
    assert_eq!(cookie.name(), COOKIE_NAME);
    assert!(cookie.secure().unwrap_or(false), "{cookie:?}");
    assert!(cookie.http_only().unwrap_or(false), "{cookie:?}");
    assert_eq!(cookie.path(), Some("/"));
    assert!(cookie.domain().is_none(), "{cookie:?}");
    assert!(
        matches!(
            cookie.same_site(),
            Some(topcoat::cookie::SameSite::Lax) | None
        ),
        "{cookie:?}"
    );
}
