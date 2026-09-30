use super::*;

fn response() -> Response {
    Response::builder().body(Body::empty()).unwrap()
}

#[test]
fn default_directive_is_self() {
    // The exact literal the browser receives. Comparing against a
    // `#[cfg(test)]` re-implementation of the same `format!` cannot fail
    // both sides would change together.
    let mut response = response();
    insert_frame_ancestors(&mut response, &FrameAncestors::same_origin().directive);
    assert_eq!(
        response.headers().get(&CSP).unwrap(),
        "frame-ancestors 'self'"
    );
}

#[test]
fn an_existing_policy_wins() {
    let mut response = response();
    response
        .headers_mut()
        .insert(CSP, header::HeaderValue::from_static("default-src 'none'"));
    insert_frame_ancestors(&mut response, "'self'");
    assert_eq!(
        response.headers().get(&CSP).unwrap(),
        "default-src 'none'",
        "an app policy must not be overwritten (or duplicated)"
    );
}

#[test]
fn an_invalid_directive_is_dropped_not_panicked() {
    // A newline would split the header; the layer must not panic or emit a
    // truncated value.
    let mut response = response();
    insert_frame_ancestors(&mut response, "'self'\r\nX-Evil: 1");
    assert!(response.headers().get(&CSP).is_none());
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
    // `FrameAncestors` this layer does not defer to what is there. The
    // literal is the directive the browser must receive: `frame-ancestors`
    // is here because `FrameAncestors` skips a response that already has a
    // policy (GH #216: comparing against the implementation constant would
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
