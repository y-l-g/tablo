//! Drives a built `Router` without a socket, carrying cookies, CSRF token, and
//! tenant. Body and HTML helpers live in `tablo_core::protocol` and are
//! re-exported here.

use http::header::{CONTENT_TYPE, COOKIE};
pub use tablo_core::protocol::{
    FilterOption, Row, RowActions, SESSION_COOKIE, body_bytes, body_string, cookie_header,
    field_error, filter_options, form_body, input_value, multipart_body, response_cookies,
    row_actions, rows, session_cookie_value, set_cookie_header,
};
use topcoat::router::{Body, Router};

/// Sends requests carrying cookies and tenant.
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

    /// Attaches a cookie to every request; a later value for the same name
    /// replaces the earlier one.
    pub fn cookie(&self, name: &str, value: &str) -> Self {
        let mut client = self.clone();
        match client.cookies.iter_mut().find(|(kept, _)| kept == name) {
            Some(existing) => existing.1 = value.to_string(),
            None => client.cookies.push((name.to_string(), value.to_string())),
        }
        client
    }

    /// Attaches every `(name, value)` pair, replacing earlier values for the
    /// same name.
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

    /// Scopes the request to another tenant, taking precedence over the
    /// logged-in user's tenant.
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

    pub async fn post_form(&self, uri: &str, body: String) -> http::Response<Body> {
        let mut request = self.request(http::Method::POST, uri);
        request.headers_mut().insert(
            CONTENT_TYPE,
            http::HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        *request.body_mut() = Body::from(body);
        self.router.handle(request).await
    }

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

    /// Posts JSON with the page identity header the browser runtime sends.
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
