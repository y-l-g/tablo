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
    assert!(verify(&cx, Some(&mask(&token))).is_ok());
    assert!(verify(&cx, Some(&mask(&uuid::Uuid::new_v4().to_string()))).is_err());
    assert!(verify(&cx, None).is_err());
    assert!(verify(&cx_with_cookie(COOKIE_NAME, None), Some(&mask(&token))).is_err());
}

/// A page never renders the raw token, so a post carrying it is not one a page produced.
#[test]
fn verify_refuses_the_unmasked_token() {
    let token = uuid::Uuid::new_v4().to_string();
    let cx = cx_with_cookie(COOKIE_NAME, Some(&token));
    assert!(verify(&cx, Some(&token)).is_err());
}

#[test]
fn mask_refuses_what_is_not_a_token() {
    assert_eq!(mask(""), "");
    assert_eq!(mask("not-a-uuid"), "");
}

proptest::proptest! {
    /// Every mask of the cookie's token verifies, no two renders share their bytes, and a mask
    /// with any byte changed fails.
    #[test]
    fn each_mask_verifies_and_differs(bytes: [u8; 16], flip in 0usize..32, bit in 0u8..8) {
        let token = uuid::Uuid::from_bytes(bytes).to_string();
        let cx = cx_with_cookie(COOKIE_NAME, Some(&token));
        let (first, second) = (mask(&token), mask(&token));
        proptest::prop_assert_ne!(&first, &second);
        proptest::prop_assert!(verify(&cx, Some(&first)).is_ok());
        proptest::prop_assert!(verify(&cx, Some(&second)).is_ok());

        let mut tampered = BASE64URL_NOPAD.decode(first.as_bytes()).unwrap();
        tampered[flip] ^= 1 << bit;
        let tampered = BASE64URL_NOPAD.encode(&tampered);
        proptest::prop_assert!(verify(&cx, Some(&tampered)).is_err());
        proptest::prop_assert!(verify(&cx, Some(&first[..first.len() - 1])).is_err());
    }
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
