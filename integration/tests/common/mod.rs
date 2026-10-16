use http::header::{CONTENT_SECURITY_POLICY, CONTENT_TYPE, COOKIE};
pub use tablo::testing::{
    TestClient, body_bytes, body_string, confirms_first, cookie_header, field_error,
    field_error_message, filter_options, form_body, input_value, mount, refusal, response_cookies,
    rows, set_cookie_header,
};
use tablo::{Auth, Notification, Panel, Resource};
use toasty::Db;
use topcoat::router::{Body, Router, response::Response};
use uuid::Uuid;

/// The opening tag of the control posting `name`.
pub fn control<'h>(html: &'h str, name: &str) -> &'h str {
    let needle = format!("name=\"{name}\"");
    let at = html
        .find(&needle)
        .unwrap_or_else(|| panic!("no {name} control in {html}"));
    let start = html[..at].rfind('<').unwrap();
    &html[start..at + html[at..].find('>').unwrap()]
}

pub async fn memory_db(models: toasty::schema::ModelSet) -> Db {
    let db = Db::builder()
        .models(models)
        .connect("sqlite::memory:")
        .await
        .expect("connect");
    db.push_schema().await.expect("push_schema");
    db
}

pub fn panel() -> Panel {
    Panel::new("admin").auth(Auth::disabled())
}

pub fn router_with<R: Resource>(db: Db, auth: Auth) -> Router {
    mount(db, Panel::new("admin").auth(auth).resource::<R>()).expect("panel builds")
}

pub fn panel_router<R: Resource>(db: Db) -> Router {
    mount(db, panel().resource::<R>()).expect("panel builds")
}

/// A context outside any request where `R` answers as a panel mounting only it would.
pub fn panel_cx<R: Resource>(db: &Db) -> topcoat::context::Cx {
    panel()
        .resource::<R>()
        .handle(db)
        .expect("panel builds")
        .context()
}

pub async fn get(router: &Router, uri: &str) -> Response<Body> {
    let request = http::Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request builds");
    router.handle(request).await
}

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

/// Posts `fields` as a panel form does, with its CSRF pair.
pub async fn post_fields(router: &Router, uri: &str, fields: &[(&str, &str)]) -> Response<Body> {
    TestClient::new(router)
        .submit(uri, &form_body(fields))
        .await
}

/// The flash notification a response set for the page it redirects to.
pub fn flash(response: &Response<Body>) -> Notification {
    tablo::testing::notification(response).expect("the response sets a flash notification")
}

pub fn new_csrf() -> String {
    Uuid::new_v4().to_string()
}

pub fn csp(response: &Response<Body>) -> &str {
    response
        .headers()
        .get(CONTENT_SECURITY_POLICY)
        .expect("response carries a policy")
        .to_str()
        .expect("the policy is ASCII")
}
