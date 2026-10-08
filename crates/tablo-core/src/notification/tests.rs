use super::*;
use crate::test_support::cx_with_cookie;

/// topcoat#441: the shard is served at the named path, so its endpoint is the
/// same in every build.
#[test]
fn live_toaster_endpoint_is_the_named_path() {
    use topcoat::router::Route as _;

    assert_eq!(live_toaster.path().as_str(), LIVE_TOASTER_PATH);
}

/// A failed write flashes an error the user can read — the
/// operation, not the driver's error text.
#[test]
fn write_failure_notification_names_the_operation() {
    let cx = cx_with_cookie(COOKIE_NAME, None);
    notify_write_failure(&cx, "create the record");
    let notification = take_notification(&cx).expect("a failed write must flash");
    assert_eq!(notification.status, NotificationStatus::Error);
    assert!(
        notification.title.contains("create the record"),
        "the toast must name the operation, got {:?}",
        notification.title
    );
    assert!(
        notification.description.is_some(),
        "the toast must say the write did not land"
    );
}

/// The flash cookie decodes as JSON, percent-encoded or not, with or without the description an
/// older cookie lacks.
#[test]
fn the_flash_cookie_decodes_every_spelling_it_is_written_in() {
    let decode = |value: &str| take_notification(&cx_with_cookie(COOKIE_NAME, Some(value)));

    let plain = decode(r#"{"status":"success","title":"Created"}"#).expect("plain JSON decodes");
    assert_eq!(plain.status, NotificationStatus::Success);
    assert_eq!(plain.title, "Created");
    assert!(
        plain.description.is_none(),
        "a pre-description cookie decodes"
    );

    let encoded = decode("%7B%22status%22%3A%22success%22%2C%22title%22%3A%22Created%22%7D")
        .expect("percent-encoded JSON decodes");
    assert_eq!(encoded.title, "Created");

    let described =
        serde_json::to_string(&Notification::warning("Careful").description("Low disk")).unwrap();
    let described = decode(&described).expect("a described notification round-trips");
    assert_eq!(described.status, NotificationStatus::Warning);
    assert_eq!(described.description.as_deref(), Some("Low disk"));
}

/// The flash cookie carries the hardened `__Host-` contract:
/// `Secure`, `HttpOnly`, `SameSite=Lax`, `Path=/`, no `Domain`.
#[test]
fn notification_cookie_is_host_prefixed_and_secure() {
    let cx = cx_with_cookie(COOKIE_NAME, None);
    set_notification(&cx, Notification::success("hello"));
    let cookie = cookies(&cx)
        .get(COOKIE_NAME)
        .expect("set_notification set the cookie");
    assert_eq!(cookie.name(), COOKIE_NAME);
    // The committed value is the JSON wire format (lowercase status
    // tokens).
    assert_eq!(cookie.value(), r#"{"status":"success","title":"hello"}"#);
    assert!(cookie.secure().unwrap_or(false), "{cookie:?}");
    assert!(cookie.http_only().unwrap_or(false), "{cookie:?}");
    assert_eq!(cookie.path(), Some("/"));
    assert!(cookie.domain().is_none(), "{cookie:?}");
}

/// The `Set-Cookie` that expires the flash cookie once `cx` read it, asserted to satisfy the
/// `__Host-` contract: a removal without `Secure` is ignored by browsers, so the flash would
/// survive every navigation.
fn host_prefixed_removal(cx: &Cx) -> String {
    let mut headers = http::HeaderMap::new();
    topcoat::cookie::write_cookies(cx, &mut headers);
    let cleared = headers
        .get_all(http::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&format!("{COOKIE_NAME}=")))
        .expect("the flash cookie must be cleared")
        .to_string();
    assert!(
        cleared.contains("Max-Age=0") || cleared.contains("Expires=Thu, 01 Jan 1970"),
        "the removal must expire the cookie: {cleared}"
    );
    assert!(
        cleared.contains("Secure") && cleared.contains("Path=/"),
        "the removal must satisfy the __Host- contract: {cleared}"
    );
    assert!(
        cleared
            .split(';')
            .all(|attr| !attr.trim().starts_with("Domain=")),
        "a `__Host-` cookie must not carry a Domain: {cleared}"
    );
    cleared
}

/// A read flash cookie is cleared, so the toast shows once; the create and edit flows reach this
/// through `mutation_redirect_carries_the_flash_cookie_instead_of_a_query`.
#[test]
fn a_read_flash_cookie_is_cleared_with_a_host_prefixed_removal() {
    let enc = serde_json::to_string(&Notification::success("hello")).unwrap();
    let cx = cx_with_cookie(COOKIE_NAME, Some(&enc));
    assert!(take_notification(&cx).is_some());
    host_prefixed_removal(&cx);
}

/// An unreadable cookie is expired rather than toasted.
#[test]
fn an_unreadable_flash_cookie_is_cleared_without_a_toast() {
    let cx = cx_with_cookie(COOKIE_NAME, Some("not-json"));
    assert!(take_notification(&cx).is_none(), "garbage yields no toast");
    host_prefixed_removal(&cx);
}
