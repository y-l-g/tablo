//! Response hardening headers.
//!
//! The panel serves one document per request, and a document that anyone can
//! frame is a clickjacking surface on every deployment by default. `Panel`
//! installs [`FrameAncestors`] unless the app opts out, so the threat is closed
//! where it lands rather than in each deployment's proxy config.
//!
//! A served directory shares the panel's origin, so `Panel` also installs
//! [`ServedFileHeaders`] on each one: the files an app accepts from
//! its users are inert, whatever their extension.

use http::{StatusCode, header};
use topcoat::{
    context::Cx,
    router::{
        Body, Layer, LayerFuture, Next, Path, PathBuf,
        response::{Response, response_headers},
    },
};

use super::build::route_path;

/// Response header carrying the policy.
const CSP: header::HeaderName = header::CONTENT_SECURITY_POLICY;

/// The panel's default directive: only the panel may frame itself.
pub(crate) const DEFAULT_FRAME_ANCESTORS: &str = "'self'";

/// Emits `Content-Security-Policy: frame-ancestors <directive>` on every
/// response the panel's layer chain produces, including the router's own 404
/// and 405.
///
/// `frame-ancestors` is the one CSP directive a `<meta>` tag cannot express, so
/// it has to ride the response — which is also why it belongs here and not in
/// [`render_document`](super::shell::Panel::render_document)'s markup.
///
/// A handler's response is hardened in place, and skipped when it already
/// carries a policy, so on the `Ok` path an app that sets its own policy (its
/// own layer or route) wins. An `Err` has no response yet — the router builds
/// the 404 or 405 after the layers have returned — so the directive is queued
/// through the router's [`response_headers`] slot instead, the mechanism the
/// cookie layer uses to put `Set-Cookie` on an error response; the queue
/// appends, so a layer outside this one that turns the error into a response
/// carrying its own policy emits two `Content-Security-Policy` headers.
///
/// The router builds three responses outside every registered layer, so no
/// layer can harden them: the origin layer's 403 for a cross-site request, the
/// 400 for a malformed `x-topcoat-identity` header, and the bare 500 it answers
/// a panic with.
#[derive(Debug, Clone)]
pub(crate) struct FrameAncestors {
    directive: String,
}

impl FrameAncestors {
    pub(crate) fn new(directive: impl Into<String>) -> Self {
        Self {
            directive: directive.into(),
        }
    }

    /// `frame-ancestors 'self'` — the default the panel ships.
    #[cfg(test)]
    pub(crate) fn same_origin() -> Self {
        Self::new(DEFAULT_FRAME_ANCESTORS)
    }
}

impl Layer for FrameAncestors {
    fn path(&self) -> Option<&Path> {
        // No path scope: the panel prefix is not the only thing worth
        // hardening — a 404 or a login redirect is frameable too, and a
        // path-less layer is the only one that sees unmatched routes.
        None
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        Box::pin(async move {
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

/// Queue the directive for the error response the router builds after the
/// layers have returned.
///
/// There is no response to inspect on this path, so the header is appended
/// rather than inserted: the router's own error responses carry no policy for
/// it to defer to. A layer outside this one that turns the error into a
/// response carrying its own policy therefore emits two
/// `Content-Security-Policy` headers.
fn queue_frame_ancestors(cx: &Cx, directive: &str) {
    if let Ok(value) = header::HeaderValue::from_str(&format!("frame-ancestors {directive}")) {
        response_headers(cx).append(CSP, value);
    }
}

/// Add the directive unless the response already carries a policy.
fn insert_frame_ancestors(response: &mut Response, directive: &str) {
    if response.headers().contains_key(&CSP) {
        return;
    }
    // The directive is app-supplied text (a header value, not markup): a value
    // the header codec rejects is dropped rather than allowed to panic or
    // truncate a response mid-stream.
    if let Ok(value) = header::HeaderValue::from_str(&format!("frame-ancestors {directive}")) {
        response.headers_mut().insert(CSP, value);
    }
}

/// The one policy every served file carries.
///
/// `frame-ancestors 'self'` rides along because [`FrameAncestors`] only fills a
/// gap: it runs outside this layer and skips a response that already has a
/// policy, so this one has to speak for itself.
const SERVED_FILE_CSP: &str = "default-src 'none'; img-src 'self'; media-src 'self'; \
     style-src 'unsafe-inline'; sandbox; frame-ancestors 'self'";

/// The content types a served file may render inline.
///
/// Everything here is passive: no script, no markup, no plugin. The list is the
/// policy's source of truth, so widening it is the one way to re-open a type.
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

/// Emits `X-Content-Type-Options: nosniff`, a fixed sandboxing
/// `Content-Security-Policy` and `Content-Disposition: attachment` on every file
/// response the directory route serves.
///
/// A served directory shares the panel's origin (ADR-0017 makes it public by
/// decision), and Topcoat derives `Content-Type` from the file extension, so a
/// user who uploads an `.html` or `.svg` document otherwise runs script with the
/// admin's session. The policy is fixed rather than configurable: an app that
/// serves active documents mounts them on its own origin. A disposition that
/// already downloads is kept, so a route that names a file keeps its filename.
///
/// The directory route's own failures (404, 405) leave through `Err` and skip
/// this layer; they render Topcoat's plain error response, which carries no file
/// content.
#[derive(Debug, Clone)]
pub(crate) struct ServedFileHeaders {
    path: PathBuf,
}

impl ServedFileHeaders {
    /// Wraps the directory route mounted at `pattern`, the same path
    /// [`Panel::serve_dir`](super::Panel::serve_dir) registers.
    pub(crate) fn new(pattern: &str) -> Self {
        Self {
            path: route_path(pattern),
        }
    }
}

impl Layer for ServedFileHeaders {
    fn path(&self) -> Option<&Path> {
        // A route's own path is a prefix of itself, so this wraps exactly the
        // served directory: the panel's own pages keep their policy.
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

/// Make one file response the directory route served inert.
///
/// The policy overwrites whatever the directory route sent: a served file never
/// needs a scriptable policy. `nosniff` and the policy apply to every file
/// response. The disposition depends on the content type, so it is skipped on a
/// `304 Not Modified`, which carries no content type and no body to render.
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

/// Whether `content_type` may render inline: the allow-list entry, with any
/// parameters (`; charset=..`) stripped and the type lowercased. A missing or
/// unreadable type downloads rather than rendering.
fn is_inline(content_type: Option<&header::HeaderValue>) -> bool {
    let Some(mime) = content_type.and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let mime = mime.split(';').next().unwrap_or_default().trim();
    INLINE_TYPES.contains(&mime.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests;
