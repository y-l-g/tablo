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

#[test]
fn take_notification_decodes_the_json_cookie() {
    let enc = serde_json::to_string(&Notification::success("hello")).unwrap();
    let cx = cx_with_cookie(COOKIE_NAME, Some(&enc));
    let n = take_notification(&cx);
    assert!(n.is_some(), "the JSON flash cookie decodes");
    assert_eq!(n.unwrap().title, "hello");
    // Clearing itself is pinned by
    // `notification_removal_header_carries_the_host_prefix_contract`.
}

#[test]
fn percent_encoded_json_cookie_decodes() {
    let enc = "%7B%22status%22%3A%22success%22%2C%22title%22%3A%22Created%22%7D";
    let cx = cx_with_cookie(COOKIE_NAME, Some(enc));
    let n = take_notification(&cx);
    assert!(n.is_some(), "the percent-encoded flash cookie decodes");
    assert_eq!(n.unwrap().title, "Created");
}

/// Unreadable cookie garbage is expired, not toasted, so a malformed cookie
/// yields no toast; the removal still satisfies the `__Host-` contract.
#[test]
fn unreadable_flash_cookie_is_expired_silently() {
    let cx = cx_with_cookie(COOKIE_NAME, Some("not-json"));
    assert!(take_notification(&cx).is_none(), "garbage yields no toast");
    let mut headers = http::HeaderMap::new();
    topcoat::cookie::write_cookies(&cx, &mut headers);
    let cleared = headers
        .get_all(http::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with(&format!("{COOKIE_NAME}=")))
        .expect("the garbage cookie must be expired");
    assert!(
        cleared.contains("Max-Age=0") && cleared.contains("Secure") && cleared.contains("Path=/"),
        "the removal carries the __Host- contract: {cleared}"
    );
}

#[test]
fn notification_description_round_trips() {
    // The description is optional and absent from the wire format when
    // unset (compatibility); old cookies still decode.
    assert_eq!(
        serde_json::to_string(&Notification::success("hello")).unwrap(),
        r#"{"status":"success","title":"hello"}"#
    );
    let enc =
        serde_json::to_string(&Notification::warning("Careful").description("Low disk")).unwrap();
    assert!(enc.contains("\"status\":\"warning\"") && enc.contains("\"description\":\"Low disk\""));
    let back = take_notification(&cx_with_cookie(COOKIE_NAME, Some(&enc))).expect("decodes");
    assert_eq!(back.description.as_deref(), Some("Low disk"));
    let old = take_notification(&cx_with_cookie(
        COOKIE_NAME,
        Some(r#"{"status":"success","title":"hi"}"#),
    ))
    .expect("a pre-description cookie decodes");
    assert!(old.description.is_none());

    // The status token is shared with the cookie and `data-type`.
    assert_eq!(NotificationStatus::Success.as_str(), "success");
    assert_eq!(NotificationStatus::Error.as_str(), "error");
    assert_eq!(NotificationStatus::Info.as_str(), "info");
    assert_eq!(NotificationStatus::Warning.as_str(), "warning");
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

/// The consumed flash cookie must be cleared with a `__Host-`-conformant
/// removal: a `__Host-`-named `Set-Cookie` without `Secure` is
/// ignored by browsers — `Max-Age=0` deletions included — so the flash
/// would survive every navigation. Pinned here through topcoat's own
/// response finalization; the create/edit flow end-to-end is covered by
/// the panel test `mutation_redirect_carries_the_flash_cookie_instead_of_a_query`.
#[test]
fn notification_removal_header_carries_the_host_prefix_contract() {
    use http::header::SET_COOKIE;

    let enc = serde_json::to_string(&Notification::success("hello")).unwrap();
    let cx = cx_with_cookie(COOKIE_NAME, Some(&enc));
    let n = take_notification(&cx);
    assert!(n.is_some(), "the flash cookie must decode");

    let mut headers = http::HeaderMap::new();
    topcoat::cookie::write_cookies(&cx, &mut headers);
    let cleared = headers
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with(&format!("{COOKIE_NAME}=")))
        .expect("the consumed flash cookie must be cleared")
        .to_string();
    assert!(
        cleared.contains("Max-Age=0") || cleared.contains("Expires=Thu, 01 Jan 1970"),
        "the removal header must expire the cookie: {cleared}"
    );
    assert!(
        cleared.contains("Secure") && cleared.contains("Path=/"),
        "the removal header must satisfy the __Host- contract: {cleared}"
    );
    assert!(
        cleared.split(';').all(|attr| {
            let attr = attr.trim();
            !attr.starts_with("Domain=")
        }),
        "a `__Host-` cookie must not carry a Domain: {cleared}"
    );
}
