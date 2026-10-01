//! In-memory HTTP test client for Tablo panels.
//!
//! [`TestClient`] drives a built `Router` without a socket: it carries
//! cookies, a CSRF token, and a tenant, and the helpers below write form and
//! multipart bodies and read values back out of the rendered HTML. An app
//! reaches it as `tablo::testing` with the facade's `testing` feature; the
//! showcase and `tablo-core` suites use the same client. Seed and database
//! fixtures stay with each suite: they name its own models.

use http::header::{CONTENT_TYPE, COOKIE};
use http_body_util::BodyExt;
use topcoat::router::{Body, Router};

/// One request client for every suite.
///
/// Every request is built here, so a suite attaches cookies, a tenant, or a
/// session in one place. Builder methods clone the client, leaving the base
/// reusable: `client.tenant(t).csrf(&token).post_form(uri, body)`.
#[derive(Clone)]
pub struct TestClient<'a> {
    router: &'a Router,
    cookies: Vec<(String, String)>,
    tenant: Option<uuid::Uuid>,
}

impl<'a> TestClient<'a> {
    pub fn new(router: &'a Router) -> Self {
        Self {
            router,
            cookies: Vec::new(),
            tenant: None,
        }
    }

    /// Attach a cookie to every request this client sends. A later value for
    /// the same name replaces the earlier one, like a browser jar.
    pub fn cookie(&self, name: &str, value: &str) -> Self {
        let mut client = self.clone();
        match client.cookies.iter_mut().find(|(kept, _)| kept == name) {
            Some(existing) => existing.1 = value.to_string(),
            None => client.cookies.push((name.to_string(), value.to_string())),
        }
        client
    }

    /// Attach every `(name, value)` pair, e.g. the cookies a response set.
    /// Like a browser jar: a later value for the same name replaces the
    /// earlier one instead of appending a duplicate `Cookie` entry.
    pub fn cookies(&self, cookies: &[(String, String)]) -> Self {
        let mut client = self.clone();
        for (name, value) in cookies {
            match client.cookies.iter_mut().find(|(kept, _)| kept == name) {
                Some(existing) => existing.1 = value.clone(),
                None => client.cookies.push((name.clone(), value.clone())),
            }
        }
        client
    }

    /// Attach the CSRF cookie the form's `csrf_token` field must match.
    pub fn csrf(&self, token: &str) -> Self {
        self.cookie(tablo_core::csrf::COOKIE_NAME, token)
    }

    /// Carry a tenant as a `Tenant` request extension — the server-set
    /// override seam. It takes precedence over the logged-in user's
    /// tenant, letting a suite scope one request to another tenant.
    pub fn tenant(&self, tenant: uuid::Uuid) -> Self {
        let mut client = self.clone();
        client.tenant = Some(tenant);
        client
    }

    pub async fn get(&self, uri: &str) -> http::Response<Body> {
        self.router
            .handle(self.request(http::Method::GET, uri))
            .await
    }

    /// POST an urlencoded form.
    pub async fn post_form(&self, uri: &str, body: String) -> http::Response<Body> {
        let mut request = self.request(http::Method::POST, uri);
        request.headers_mut().insert(
            CONTENT_TYPE,
            http::HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        *request.body_mut() = Body::from(body);
        self.router.handle(request).await
    }

    /// POST a multipart body (file uploads).
    pub async fn post_multipart(
        &self,
        uri: &str,
        boundary: &str,
        body: String,
    ) -> http::Response<Body> {
        let mut request = self.request(http::Method::POST, uri);
        request.headers_mut().insert(
            CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}")
                .parse()
                .expect("multipart content type"),
        );
        *request.body_mut() = Body::from(body);
        self.router.handle(request).await
    }

    /// POST a JSON body to a runtime endpoint (a shard or procedure), with the
    /// page identity header the browser runtime sends.
    pub async fn post_json(&self, uri: &str, body: String, identity: &str) -> http::Response<Body> {
        let mut request = self.request(http::Method::POST, uri);
        request.headers_mut().insert(
            CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        request.headers_mut().insert(
            topcoat::router::request::IDENTITY_HEADER,
            http::HeaderValue::from_str(identity).expect("identity header"),
        );
        *request.body_mut() = Body::from(body);
        self.router.handle(request).await
    }

    /// Build a request carrying this client's cookies and tenant.
    fn request(&self, method: http::Method, uri: &str) -> http::Request<Body> {
        let mut builder = http::Request::builder().method(method).uri(uri);
        if let Some(jar) = cookie_header(
            self.cookies
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        ) {
            builder = builder.header(COOKIE, jar);
        }
        let (mut parts, body) = builder.body(Body::empty()).unwrap().into_parts();
        if let Some(tenant) = self.tenant {
            parts.extensions.insert(tablo_core::Tenant(tenant));
        }
        http::Request::from_parts(parts, body)
    }
}

/// One `Cookie` header value from `(name, value)` pairs, or `None` when there
/// are no pairs to send.
pub fn cookie_header<'a>(cookies: impl IntoIterator<Item = (&'a str, &'a str)>) -> Option<String> {
    let mut jar = String::new();
    for (name, value) in cookies {
        if !jar.is_empty() {
            jar.push_str("; ");
        }
        jar.push_str(name);
        jar.push('=');
        jar.push_str(value);
    }
    (!jar.is_empty()).then_some(jar)
}

/// Collect a response body as bytes.
pub async fn body_bytes(response: http::Response<Body>) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes()
        .to_vec()
}

/// Collect a response body as a lossy UTF-8 string.
pub async fn body_string(response: http::Response<Body>) -> String {
    String::from_utf8_lossy(&body_bytes(response).await).into_owned()
}

/// URL-encode `(key, value)` pairs into an urlencoded form body.
pub fn form_body(pairs: &[(&str, &str)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

/// A multipart body, one part per entry: `None` is a text part, `Some("")` the
/// browser's "no file chosen" file part, `Some(name)` a chosen file.
///
/// Parts carry no per-part `Content-Type`: neither server parser reads one.
/// The framework parser (`tablo_core` multipart values, over Topcoat's
/// multer-based extractor) tells file parts from text parts by the
/// `filename` parameter alone, and the showcase media upload reads the part's
/// content type only as a display-kind hint defaulting to file. A suite that
/// needs a part content type (the media kind cases) builds that body ad-hoc.
pub fn multipart_body(boundary: &str, parts: &[(&str, Option<&str>, &str)]) -> String {
    let mut body = String::new();
    for (name, filename, content) in parts {
        body.push_str(&format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\""
        ));
        if let Some(filename) = filename {
            body.push_str(&format!("; filename=\"{filename}\""));
        }
        body.push_str("\r\n\r\n");
        body.push_str(content);
        body.push_str("\r\n");
    }
    body.push_str(&format!("--{boundary}--\r\n"));
    body
}

/// The `value` attribute of the named `<input>` in rendered HTML, in either
/// attribute order.
///
/// Handles both quote styles (`value="…"` and `value='…'`); the controlled
/// UUID markup only emits double quotes today, but an encoder change must
/// not silently turn every lookup into `None` (harness hardening).
pub fn input_value(html: &str, name: &str) -> Option<String> {
    let double = format!("name=\"{name}\"");
    let single = format!("name='{name}'");
    for tag in html.split('<').skip(1) {
        if !tag.contains(&double) && !tag.contains(&single) {
            continue;
        }
        let attrs = &tag[..tag.find('>')?];
        for (prefix, term) in [("value=\"", '"'), ("value='", '\'')] {
            if let Some(start) = attrs.find(prefix) {
                let rest = &attrs[start + prefix.len()..];
                if let Some(end) = rest.find(term) {
                    // Unescape the matching quote entity the encoder may emit.
                    let raw = &rest[..end];
                    return Some(raw.replace("&quot;", "\"").replace("&#x27;", "'"));
                }
            }
        }
    }
    None
}

/// The session cookie name Topcoat's default token store writes (`__Host-`
/// prefix plus the `session` name, per its hardened cookie contract).
pub const SESSION_COOKIE: &str = "__Host-session";

/// The `(name, value)` pairs a response's `Set-Cookie` headers carry.
pub fn response_cookies(response: &http::Response<Body>) -> Vec<(String, String)> {
    response
        .headers()
        .get_all(http::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .filter_map(|pair| pair.split_once('='))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect()
}

/// The full `Set-Cookie` header for `name`, so tests can assert attributes.
pub fn set_cookie_header(response: &http::Response<Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get_all(http::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&format!("{name}=")))
        .map(str::to_string)
}

/// The session cookie value a response set, if any.
pub fn session_cookie_value(response: &http::Response<Body>) -> Option<String> {
    response_cookies(response)
        .into_iter()
        .find(|(name, _)| name == SESSION_COOKIE)
        .map(|(_, value)| value)
}
