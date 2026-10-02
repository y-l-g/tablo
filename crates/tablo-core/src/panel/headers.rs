//! Response hardening headers for panel responses and served files.

use http::{StatusCode, header};
use topcoat::{
    context::Cx,
    router::{
        Body, Layer, LayerFuture, Next, Path, PathBuf,
        request::uri,
        response::{Response, response_headers},
    },
};

use super::{build::route_path, state::under_prefix};

const CSP: header::HeaderName = header::CONTENT_SECURITY_POLICY;

/// The panel's default directive: only the panel may frame itself.
pub(crate) const DEFAULT_FRAME_ANCESTORS: &str = "'self'";

/// Emits `Content-Security-Policy: frame-ancestors <directive>` on responses under the panel
/// prefix.
#[derive(Debug, Clone)]
pub(crate) struct FrameAncestors {
    directive: String,
    prefix: String,
}

impl FrameAncestors {
    pub(crate) fn new(directive: impl Into<String>, prefix: impl Into<String>) -> Self {
        Self {
            directive: directive.into(),
            prefix: prefix.into(),
        }
    }
}

impl Layer for FrameAncestors {
    fn path(&self) -> Option<&Path> {
        None
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            if !under_prefix(&self.prefix, uri(cx).path()) {
                return next.run(cx, body).await;
            }
            match next.run(cx, body).await {
                Ok(mut response) => {
                    insert_frame_ancestors(&mut response, &self.directive);
                    Ok(response)
                }
                Err(error) => {
                    queue_frame_ancestors(cx, &self.directive);
                    Err(error)
                }
            }
        })
    }
}

/// Queues the directive for the error response the router builds after the layers return.
fn queue_frame_ancestors(cx: &Cx, directive: &str) {
    if let Ok(value) = header::HeaderValue::from_str(&format!("frame-ancestors {directive}")) {
        response_headers(cx).append(CSP, value);
    }
}

/// Adds the directive unless the response already carries a policy.
fn insert_frame_ancestors(response: &mut Response, directive: &str) {
    if response.headers().contains_key(&CSP) {
        return;
    }
    // Drops a directive value the header codec rejects.
    if let Ok(value) = header::HeaderValue::from_str(&format!("frame-ancestors {directive}")) {
        response.headers_mut().insert(CSP, value);
    }
}

/// The policy every served file carries.
const SERVED_FILE_CSP: &str = "default-src 'none'; img-src 'self'; media-src 'self'; \
     style-src 'unsafe-inline'; sandbox; frame-ancestors 'self'";

/// The content types a served file may render inline.
const INLINE_TYPES: &[&str] = &[
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
];

/// Hardens every file response the directory route serves.
#[derive(Debug, Clone)]
pub(crate) struct ServedFileHeaders {
    path: PathBuf,
}

impl ServedFileHeaders {
    /// Wraps the directory route mounted at `pattern`.
    pub(crate) fn new(pattern: &str) -> Self {
        Self {
            path: route_path(pattern),
        }
    }
}

impl Layer for ServedFileHeaders {
    fn path(&self) -> Option<&Path> {
        // Wraps exactly the served directory.
        Some(&self.path)
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
            let mut response = next.run(cx, body).await?;
            harden_served_file(&mut response);
            Ok(response)
        })
    }
}

/// Makes one file response the directory route served inert.
fn harden_served_file(response: &mut Response) {
    let not_modified = response.status() == StatusCode::NOT_MODIFIED;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    headers.insert(CSP, header::HeaderValue::from_static(SERVED_FILE_CSP));
    if not_modified || is_inline(headers.get(header::CONTENT_TYPE)) {
        return;
    }
    let already_an_attachment = headers
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("attachment"));
    if !already_an_attachment {
        headers.insert(
            header::CONTENT_DISPOSITION,
            header::HeaderValue::from_static("attachment"),
        );
    }
}

/// Reports whether `content_type` may render inline.
fn is_inline(content_type: Option<&header::HeaderValue>) -> bool {
    let Some(mime) = content_type.and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let mime = mime.split(';').next().unwrap_or_default().trim();
    INLINE_TYPES.contains(&mime.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests;
