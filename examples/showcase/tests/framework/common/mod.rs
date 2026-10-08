use http::header::{CONTENT_SECURITY_POLICY, CONTENT_TYPE, COOKIE};
pub use tablo::testing::{
    body_bytes, body_string, cookie_header, field_error, filter_options, input_value,
    multipart_body, response_cookies, rows,
};
use tablo::{
    Auth, DeclarationError, MountError, Notification, Panel, Resource, RouterBuilderPanelExt,
};
use toasty::Db;
use topcoat::router::{Body, Router, RouterBuilderDiscoverExt, response::Response};
use uuid::Uuid;

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

pub fn mount(db: Db, panel: Panel) -> topcoat::Result<Router> {
    Ok(Router::builder()
        .discover()
        .app_context(db)
        .panel(panel)?
        .build())
}

/// The mistakes a panel refused to mount with.
pub fn refusal<T>(mounted: topcoat::Result<T>) -> Vec<DeclarationError> {
    let Err(error) = mounted else {
        panic!("the panel must not mount");
    };
    error
        .downcast_ref::<MountError>()
        .unwrap_or_else(|| panic!("a declaration mistake refuses the panel, got {error}"))
        .errors()
        .to_vec()
}

pub fn router_with<R: Resource>(db: Db, auth: Auth) -> Router {
    mount(db, Panel::new("admin").auth(auth).resource::<R>()).expect("panel builds")
}

pub fn panel_router<R: Resource>(db: Db) -> Router {
    mount(db, panel().resource::<R>()).expect("panel builds")
}

/// A context outside any request where `R` answers as a panel mounting only it would.
pub fn panel_cx<R: Resource>(db: &Db) -> topcoat::context::Cx {
    panel().resource::<R>().context(db).expect("panel builds")
}

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
        .header(COOKIE, format!("{}={csrf}", tablo::csrf::COOKIE_NAME))
        .body(Body::from(body))
        .expect("request builds");
    router.handle(request).await
}

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

/// Whether the control posting to `action` asks first: a confirming trigger is a plain button that
/// opens the dialog, a direct write is a submit button. `None` when no control posts there.
pub fn confirms_first(html: &str, action: &str) -> Option<bool> {
    let target = format!("formaction=\"{action}\"");
    let at = html.find(&target)?;
    let start = html[..at].rfind("<button")?;
    // Attribute values carry `>` (event handlers), so the tag ends at the first `>` outside quotes.
    let mut quoted = false;
    let end = html[start..].find(|c: char| {
        if c == '"' {
            quoted = !quoted;
        }
        c == '>' && !quoted
    })?;
    Some(html[start..start + end].contains("type=\"button\""))
}
