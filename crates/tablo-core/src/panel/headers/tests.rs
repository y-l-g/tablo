use super::*;

fn response() -> Response {
    Response::builder().body(Body::empty()).unwrap()
}

fn layer(csp: Option<&str>) -> PanelHeaders {
    PanelHeaders::new("/admin", csp, None).unwrap()
}

#[test]
fn a_header_the_handler_set_wins() {
    let mut response = response();
    response
        .headers_mut()
        .insert(CSP, header::HeaderValue::from_static("default-src 'none'"));
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        header::HeaderValue::from_static("no-referrer"),
    );
    insert_missing(&mut response, &layer(Some(DEFAULT_CSP)).headers);
    let headers = response.headers();
    assert_eq!(headers.get(&CSP).unwrap(), "default-src 'none'");
    assert_eq!(headers.get(header::REFERRER_POLICY).unwrap(), "no-referrer");
    assert_eq!(headers.get_all(&CSP).iter().count(), 1, "never duplicated");
    assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
}

#[test]
fn a_policy_with_a_line_break_is_a_declaration_error() {
    let error = PanelHeaders::new("/admin", Some("'self'\r\nX-Evil: 1"), None).unwrap_err();
    assert!(matches!(
        error,
        crate::DeclarationErrorKind::InvalidContentSecurityPolicy { .. }
    ));
}

#[test]
fn hsts_is_sent_only_when_asked() {
    let hsts = |max_age| {
        PanelHeaders::new("/admin", None, max_age)
            .unwrap()
            .headers
            .into_iter()
            .find(|(name, _)| name == header::STRICT_TRANSPORT_SECURITY)
            .map(|(_, value)| value)
    };
    assert_eq!(hsts(None), None);
    assert_eq!(
        hsts(Some(std::time::Duration::from_secs(31_536_000))).unwrap(),
        "max-age=31536000"
    );
}

#[test]
fn the_dev_server_origin_may_load_scripts_and_connect() {
    let policy = default_csp(Some("http://127.0.0.1:3001/base"));
    assert!(
        policy.contains("script-src 'self' 'unsafe-inline' 'unsafe-eval' http://127.0.0.1:3001;"),
        "{policy}"
    );
    assert!(
        policy.contains("connect-src 'self' http://127.0.0.1:3001 ws://127.0.0.1:3001;"),
        "{policy}"
    );
    assert_eq!(default_csp(None), DEFAULT_CSP);
}

#[test]
fn a_dev_url_that_is_no_http_origin_adds_nothing() {
    for url in [
        "javascript:alert(1)",
        "http://",
        "http://host; script-src *",
        "ftp://host",
        "localhost:3001",
    ] {
        assert_eq!(default_csp(Some(url)), DEFAULT_CSP, "{url}");
    }
}

/// A served file's response with `content_type`, or none at all.
fn file_response(content_type: Option<&str>) -> Response {
    let mut response = response();
    if let Some(content_type) = content_type {
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_str(content_type).unwrap(),
        );
    }
    response
}

fn policy(response: &Response) -> &str {
    response.headers().get(&CSP).unwrap().to_str().unwrap()
}

fn disposition(response: &Response) -> Option<&str> {
    response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .map(|value| value.to_str().unwrap())
}

#[test]
fn the_allow_list_renders_inline() {
    for content_type in [
        "image/png",
        "image/jpeg",
        "image/gif",
        "image/webp",
        "image/avif",
        "video/mp4",
        "video/webm",
        "audio/mpeg",
        "audio/ogg",
        "audio/wav",
        "text/plain",
        // The type is case-insensitive and parameters are not part of it.
        "TEXT/PLAIN; charset=utf-8",
    ] {
        let mut response = file_response(Some(content_type));
        harden_served_file(&mut response);
        assert_eq!(
            response
                .headers()
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .unwrap(),
            "nosniff",
            "{content_type} must not be sniffed"
        );
        assert!(
            policy(&response).contains("sandbox"),
            "{content_type} must carry the sandbox policy"
        );
        assert_eq!(
            disposition(&response),
            None,
            "{content_type} renders inline"
        );
    }
}

#[test]
fn everything_else_downloads() {
    // `image/svg+xml` can script, `text/html` is a document, and a PDF is a
    // plugin surface; the browser must save rather than render them.
    for content_type in [
        "image/svg+xml",
        "text/html; charset=utf-8",
        "application/pdf",
    ] {
        let mut response = file_response(Some(content_type));
        harden_served_file(&mut response);
        assert_eq!(
            disposition(&response),
            Some("attachment"),
            "{content_type} must download"
        );
        assert!(
            policy(&response).contains("sandbox"),
            "{content_type} must carry the sandbox policy"
        );
    }
}

#[test]
fn a_missing_content_type_downloads() {
    let mut response = file_response(None);
    harden_served_file(&mut response);
    assert_eq!(disposition(&response), Some("attachment"));
}

#[test]
fn an_existing_disposition_is_kept_only_when_it_downloads() {
    let mut response = file_response(Some("text/html"));
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_static("attachment; filename=\"report.html\""),
    );
    harden_served_file(&mut response);
    assert_eq!(
        disposition(&response),
        Some("attachment; filename=\"report.html\""),
        "the directory route's own filename survives"
    );

    let mut response = file_response(Some("text/html"));
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_static("inline"),
    );
    harden_served_file(&mut response);
    assert_eq!(
        disposition(&response),
        Some("attachment"),
        "an inline disposition is replaced, never left to render"
    );
}

#[test]
fn a_not_modified_response_carries_the_policy_and_no_disposition() {
    // A 304 has no `Content-Type`, so the disposition rule cannot see what
    // is being revalidated; an inline image must not flip to a download.
    let mut response = file_response(None);
    *response.status_mut() = StatusCode::NOT_MODIFIED;
    harden_served_file(&mut response);
    assert_eq!(
        response
            .headers()
            .get(header::X_CONTENT_TYPE_OPTIONS)
            .unwrap(),
        "nosniff"
    );
    assert!(policy(&response).contains("sandbox"));
    assert_eq!(disposition(&response), None);
}

#[test]
fn an_existing_policy_is_overwritten() {
    // A served file never needs a scriptable policy, so unlike
    // `PanelHeaders` this layer does not defer to what is there. The
    // literal is the directive the browser must receive: `frame-ancestors`
    // is here because `PanelHeaders` skips a response that already has a
    // policy (comparing against the implementation constant would
    // change both sides together).
    let mut response = file_response(Some("text/html"));
    response
        .headers_mut()
        .insert(CSP, header::HeaderValue::from_static("script-src *"));
    harden_served_file(&mut response);
    assert_eq!(
        policy(&response),
        "default-src 'none'; img-src 'self'; media-src 'self'; \
             style-src 'unsafe-inline'; sandbox; frame-ancestors 'self'"
    );
}
