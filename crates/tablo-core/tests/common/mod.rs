//! Shared HTTP harness for the `tablo-core` integration suite.
//!
//! One binary (`tests/it.rs`) compiles every module, so the request builders
//! and the DB builder live here instead of being redeclared per module. The
//! protocol helpers shared with the showcase suite (body readers, the
//! multipart writer, the cookie jar, `input_value`) live in `tablo-test` and
//! are re-exported below; the showcase suite's `tests/common` is the same
//! idea on the app side.
//!
//! The auth-gated `auth_override` module is the only caller of the
//! cookie-carrying helpers, so those carry the `auth` gate too.

use http::header::{CONTENT_SECURITY_POLICY, CONTENT_TYPE, COOKIE};
use tablo_core::{Auth, Panel, Resource};
#[cfg(feature = "auth")]
use tablo_test::cookie_header;
pub use tablo_test::{body_bytes, body_string, multipart_body};
#[cfg(feature = "auth")]
pub use tablo_test::{input_value, response_cookies};
use toasty::Db;
use topcoat::router::{Body, Router, response::Response};
use uuid::Uuid;

/// An in-memory SQLite `Db` with `models` registered and its schema pushed.
pub async fn memory_db(models: toasty::schema::ModelSet) -> Db {
    let db = Db::builder()
        .models(models)
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    db.push_schema().await.expect("push_schema");
    db
}

/// A panel mounted at `/admin` with the auth gate off — the shape every suite
/// here builds before adding its own resources.
pub fn panel(db: Db) -> Panel {
    Panel::new("admin").app_context(db).auth(Auth::disabled())
}

/// [`panel`] with one resource registered and built under `auth`.
#[cfg(feature = "auth")]
pub fn router_with<R: Resource>(db: Db, auth: Auth) -> Router {
    Panel::new("admin")
        .app_context(db)
        .auth(auth)
        .resource::<R>()
        .build()
        .expect("panel builds")
}

/// A router over one resource, under the disabled auth gate.
pub fn panel_router<R: Resource>(db: Db) -> Router {
    panel(db).resource::<R>().build().expect("panel builds")
}

/// A POST carrying a matching CSRF cookie + field (the double-submit pair).
pub async fn post(
    router: &Router,
    uri: &str,
    csrf: &str,
    content_type: String,
    body: String,
) -> Response<Body> {
    let request = http::Request::builder()
        .method(http::Method::POST)
        .uri(uri)
        .header(CONTENT_TYPE, content_type)
        .header(COOKIE, format!("{}={csrf}", tablo_core::csrf::COOKIE_NAME))
        .body(Body::from(body))
        .expect("request builds");
    router.handle(request).await
}

/// POST a multipart form (every upload form's enctype).
pub async fn post_multipart(
    router: &Router,
    uri: &str,
    csrf: &str,
    boundary: &str,
    body: String,
) -> Response<Body> {
    post(
        router,
        uri,
        csrf,
        format!("multipart/form-data; boundary={boundary}"),
        body,
    )
    .await
}

/// A GET with no cookies.
pub async fn get(router: &Router, uri: &str) -> Response<Body> {
    let request = http::Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request builds");
    router.handle(request).await
}

/// A GET carrying `cookies` as one `Cookie` header.
#[cfg(feature = "auth")]
pub async fn get_with_cookies(
    router: &Router,
    uri: &str,
    cookies: &[(&str, String)],
) -> Response<Body> {
    let mut request = http::Request::builder().uri(uri);
    if let Some(jar) = cookie_header(cookies.iter().map(|(name, value)| (*name, value.as_str()))) {
        request = request.header(COOKIE, jar);
    }
    router.handle(request.body(Body::empty()).unwrap()).await
}

/// A url-encoded POST carrying `cookies`.
#[cfg(feature = "auth")]
pub async fn post_form(
    router: &Router,
    uri: &str,
    cookies: &[(&str, String)],
    body: String,
) -> Response<Body> {
    let mut request = http::Request::builder()
        .method(http::Method::POST)
        .uri(uri)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(jar) = cookie_header(cookies.iter().map(|(name, value)| (*name, value.as_str()))) {
        request = request.header(COOKIE, jar);
    }
    router.handle(request.body(Body::from(body)).unwrap()).await
}

/// A url-encoded POST minting its own CSRF pair and sending `fields`.
pub async fn post_fields(router: &Router, uri: &str, fields: &[(&str, &str)]) -> Response<Body> {
    let csrf = new_csrf();
    let mut body = format!("csrf_token={csrf}");
    for (name, value) in fields {
        body.push_str(&format!("&{name}={value}"));
    }
    post(
        router,
        uri,
        &csrf,
        "application/x-www-form-urlencoded".to_string(),
        body,
    )
    .await
}

/// A new CSRF token, paired with the cookie the POST helpers send.
pub fn new_csrf() -> String {
    Uuid::new_v4().to_string()
}

/// The `Content-Security-Policy` a response carries.
pub fn csp(response: &Response<Body>) -> &str {
    response
        .headers()
        .get(CONTENT_SECURITY_POLICY)
        .expect("response carries a policy")
        .to_str()
        .expect("the policy is ASCII")
}
