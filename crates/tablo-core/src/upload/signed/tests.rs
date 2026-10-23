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
    let now = 10 * WINDOW + 7;
    let url = sign(&cx, "/uploads/a.png", now);
    assert!(url.starts_with("/uploads/a.png?"), "{url}");
    assert!(verify(&cx, "/uploads/a.png", query(&url), now));
    assert!(
        verify(&cx, "/uploads/a.png", query(&url), now + WINDOW),
        "it lasts a full window"
    );
    assert!(
        !verify(&cx, "/uploads/a.png", query(&url), now + 2 * WINDOW),
        "it expires within two windows"
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
        &format!("expires={}", 2 * WINDOW),
        &format!("expires={}", 200 * WINDOW),
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

#[test]
fn the_url_stays_the_same_within_a_window() {
    let cx = cx();
    assert_eq!(
        sign(&cx, "/uploads/a.png", 5 * WINDOW),
        sign(&cx, "/uploads/a.png", 6 * WINDOW - 1)
    );
}
