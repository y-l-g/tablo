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

use super::{Panel, state::under_prefix};
use crate::naming::route_path;

const CSP: header::HeaderName = header::CONTENT_SECURITY_POLICY;

/// The panel's default `Content-Security-Policy`, which
/// [`Panel::DEFAULT_CONTENT_SECURITY_POLICY`] documents.
pub(crate) const DEFAULT_CSP: &str = "default-src 'self'; \
     script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; \
     img-src 'self' data: blob: https:; font-src 'self' data: https:; \
     media-src 'self' data: blob: https:; object-src 'none'; base-uri 'self'; \
     form-action 'self'; frame-ancestors 'self'";

/// The `Content-Security-Policy` a panel sends.
#[derive(Debug, Clone)]
pub(crate) enum Policy {
    /// [`DEFAULT_CSP`], with the `topcoat dev` server allowed while it runs the app.
    Default,
    /// The app's own policy.
    Custom(String),
    /// No policy: the deployment's proxy owns it.
    Off,
}

impl Policy {
    /// The header value the panel sends, if any.
    pub(crate) fn resolve(self) -> Option<String> {
        match self {
            Self::Default => Some(default_csp(
                std::env::var("TOPCOAT_DEV_URL").ok().as_deref(),
            )),
            Self::Custom(policy) => Some(policy),
            Self::Off => None,
        }
    }
}

/// The default policy, with the `topcoat dev` server's origin `dev` allowed to load its live
/// reload script and connect its socket.
pub(crate) fn default_csp(dev: Option<&str>) -> String {
    let Some(dev) = dev.and_then(origin) else {
        return DEFAULT_CSP.to_owned();
    };
    // Chrome matches no `ws:` URL against an `http:` source, so the socket is named too.
    let socket = dev.replacen("http", "ws", 1);
    DEFAULT_CSP.replacen(
        "'unsafe-eval';",
        &format!("'unsafe-eval' {dev}; connect-src 'self' {dev} {socket};"),
        1,
    )
}

/// The `scheme://host[:port]` of `url`, else `None`.
fn origin(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    let valid = !host.is_empty()
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-:[]".contains(&byte));
    valid.then(|| &url[..scheme.len() + 3 + end])
}

/// The headers every response under a panel's prefix carries, unless its handler set them:
///
/// - the panel's `Content-Security-Policy`, when it has one;
/// - `X-Content-Type-Options: nosniff`, so a plain-text error that echoes input never renders as
///   HTML;
/// - `Referrer-Policy: same-origin`, so a record's URL never leaves for another site;
/// - `Cache-Control: no-store`, so a page a signed-out user saw stays out of the browser's cache
///   and its back button;
/// - `Strict-Transport-Security`, when the panel opts in.
#[derive(Debug, Clone)]
pub(crate) struct PanelHeaders {
    prefix: String,
    headers: Vec<(header::HeaderName, header::HeaderValue)>,
}

impl PanelHeaders {
    /// The headers for the panel at `prefix`; a `csp` the header codec rejects is a declaration
    /// error.
    pub(crate) fn new(
        prefix: impl Into<String>,
        csp: Option<&str>,
        hsts: Option<std::time::Duration>,
    ) -> Result<Self, crate::DeclarationErrorKind> {
        let mut headers = vec![
            (
                header::X_CONTENT_TYPE_OPTIONS,
                header::HeaderValue::from_static("nosniff"),
            ),
            (
                header::REFERRER_POLICY,
                header::HeaderValue::from_static("same-origin"),
            ),
            (
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("no-store"),
            ),
        ];
        if let Some(policy) = csp {
            let value = header::HeaderValue::from_str(policy).map_err(|_| {
                crate::DeclarationErrorKind::InvalidContentSecurityPolicy {
                    policy: policy.to_owned(),
                }
            })?;
            headers.push((CSP, value));
        }
        if let Some(max_age) = hsts {
            let value = header::HeaderValue::from_str(&format!("max-age={}", max_age.as_secs()))
                .expect("a decimal number is a valid header value");
            headers.push((header::STRICT_TRANSPORT_SECURITY, value));
        }
        Ok(Self {
            prefix: prefix.into(),
            headers,
        })
    }
}

impl Layer for PanelHeaders {
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
                    insert_missing(&mut response, &self.headers);
                    Ok(response)
                }
                Err(error) => {
                    // The router builds the error response after the layers return.
                    let queued = response_headers(cx);
                    for (name, value) in &self.headers {
                        queued.append(name.clone(), value.clone());
                    }
                    Err(error)
                }
            }
        })
    }
}

/// Adds each of `headers` that `response` does not carry yet.
fn insert_missing(response: &mut Response, headers: &[(header::HeaderName, header::HeaderValue)]) {
    let target = response.headers_mut();
    for (name, value) in headers {
        if !target.contains_key(name) {
            target.insert(name.clone(), value.clone());
        }
    }
}

impl Panel {
    /// The `Content-Security-Policy` a panel sends unless
    /// [`content_security_policy`](Self::content_security_policy) replaces it:
    ///
    /// ```text
    /// default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval';
    /// style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https:;
    /// font-src 'self' data: https:; media-src 'self' data: blob: https:; object-src 'none';
    /// base-uri 'self'; form-action 'self'; frame-ancestors 'self'
    /// ```
    ///
    /// - Scripts load from the panel's origin only, and a script can connect to it alone. Inline
    ///   scripts run: Topcoat streams a page's live regions, such as a list's rows, with inline
    ///   scripts that carry no nonce, so a policy without `'unsafe-inline'` leaves the list
    ///   loading. `'unsafe-eval'` lets Topcoat's runtime compile its client expressions.
    /// - Images, fonts and media may come from any `https:` host, so an uploader or a font on a
    ///   CDN keeps working.
    /// - Plugins are off, `<base>` and form targets stay on the origin, and only the panel may
    ///   frame itself.
    ///
    /// While `topcoat dev` runs the app, the policy also lets the dev server's origin load its
    /// reload script and connect.
    pub const DEFAULT_CONTENT_SECURITY_POLICY: &str = DEFAULT_CSP;

    /// Replaces the panel's `Content-Security-Policy`,
    /// [`DEFAULT_CONTENT_SECURITY_POLICY`](Self::DEFAULT_CONTENT_SECURITY_POLICY) by default.
    ///
    /// Every response under the prefix carries the policy, and with it `X-Content-Type-Options:
    /// nosniff`, `Referrer-Policy: same-origin` and `Cache-Control: no-store`, so a page a
    /// signed-out user saw does not come back from the browser's cache. A header a handler sets
    /// itself is kept.
    ///
    /// - The headers cover the prefix's 404 and 405 pages too; the app's own routes outside the
    ///   prefix carry whatever the app sets.
    /// - The router answers three errors before any layer runs, so they carry none of them: the
    ///   403 for a cross-site request, the 400 for a malformed `x-topcoat-identity` header, and
    ///   the 500 for a panic.
    ///
    /// A policy with a line break refuses to mount.
    ///
    /// ```no_run
    /// # use tablo_core::Panel;
    /// Panel::new("admin").content_security_policy(
    ///     Panel::DEFAULT_CONTENT_SECURITY_POLICY
    ///         .replace("frame-ancestors 'self'", "frame-ancestors 'self' https://intranet.example"),
    /// );
    /// ```
    pub fn content_security_policy(mut self, policy: impl Into<String>) -> Self {
        self.content_security_policy = Policy::Custom(policy.into());
        self
    }

    /// Sends no `Content-Security-Policy`, for deployments whose proxy owns it. The panel's other
    /// headers stay.
    pub fn without_content_security_policy(mut self) -> Self {
        self.content_security_policy = Policy::Off;
        self
    }

    /// Sends `Strict-Transport-Security: max-age=<seconds>` on every response under the prefix,
    /// so a browser that reached the panel over HTTPS never tries plain HTTP for the host again
    /// until `max_age` passes. Off by default, since TLS usually ends at a proxy, which may own
    /// the header; turn it on only for a host that serves HTTPS alone.
    ///
    /// ```no_run
    /// # use tablo_core::Panel;
    /// Panel::new("admin").strict_transport_security(std::time::Duration::from_secs(31_536_000));
    /// ```
    pub fn strict_transport_security(mut self, max_age: std::time::Duration) -> Self {
        self.strict_transport_security = Some(max_age);
        self
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
