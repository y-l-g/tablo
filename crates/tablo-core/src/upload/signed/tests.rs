use super::*;

fn cx() -> Cx {
    Cx::default()
}

fn query(url: &str) -> Option<&str> {
    url.split_once('?').map(|(_, query)| query)
}

#[test]
fn a_signed_url_verifies_for_its_path_until_it_expires() {
    let cx = cx();
    let now = 10 * LIFETIME + 7;
    let url = sign(&cx, "/uploads/a.png", now);
    assert!(url.starts_with("/uploads/a.png?"), "{url}");
    assert!(verify(&cx, "/uploads/a.png", query(&url), now));
    assert!(
        verify(&cx, "/uploads/a.png", query(&url), now + LIFETIME - 1),
        "it lasts its lifetime"
    );
    assert!(
        !verify(&cx, "/uploads/a.png", query(&url), now + LIFETIME),
        "it expires after its lifetime"
    );
}

#[test]
fn a_signature_opens_only_its_own_path() {
    let cx = cx();
    let url = sign(&cx, "/uploads/a.png", 0);
    assert!(!verify(&cx, "/uploads/b.png", query(&url), 0));
}

#[test]
fn a_forged_or_missing_signature_is_refused() {
    let cx = cx();
    let url = sign(&cx, "/uploads/a.png", 0);
    let later = url.replace(
        &format!("expires={LIFETIME}"),
        &format!("expires={}", 100 * LIFETIME),
    );
    assert_ne!(url, later);
    assert!(!verify(&cx, "/uploads/a.png", query(&later), 0));
    assert!(!verify(&cx, "/uploads/a.png", None, 0));
    assert!(!verify(
        &cx,
        "/uploads/a.png",
        Some("expires=99999999999"),
        0
    ));
}

/// The signature covers its nonce: another nonce, or none, does not carry it.
#[test]
fn a_signature_opens_only_with_its_own_nonce() {
    let cx = cx();
    let (first, second) = (
        sign(&cx, "/uploads/a.png", 0),
        sign(&cx, "/uploads/a.png", 0),
    );
    let nonce = |url: &str| {
        query(url)
            .unwrap()
            .split('&')
            .find_map(|pair| pair.strip_prefix("nonce="))
            .unwrap()
            .to_owned()
    };
    let swapped = first.replace(&nonce(&first), &nonce(&second));
    assert!(!verify(&cx, "/uploads/a.png", query(&swapped), 0));
    let dropped = first.replace(&format!("nonce={}&", nonce(&first)), "");
    assert!(!verify(&cx, "/uploads/a.png", query(&dropped), 0));
}

/// No two renders of a page hold the same URL for a file, so a compressed page gives an
/// observer no fixed signature to guess from response sizes.
#[test]
fn each_signature_differs() {
    let cx = cx();
    let (first, second) = (
        sign(&cx, "/uploads/a.png", 0),
        sign(&cx, "/uploads/a.png", 0),
    );
    assert_ne!(first, second);
    assert!(verify(&cx, "/uploads/a.png", query(&first), 0));
    assert!(verify(&cx, "/uploads/a.png", query(&second), 0));
}
