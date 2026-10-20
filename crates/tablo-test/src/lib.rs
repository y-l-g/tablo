//! Drives a built `Router` without a socket, carrying cookies, CSRF token, and
//! tenant, and reads values back out of its responses.

mod db;
mod protocol;
mod two_factor;

pub use db::{TEST_DATABASE_URL, test_db};
use http::header::{CONTENT_TYPE, COOKIE};
pub use protocol::{
    Bell, BellNotification, EmptyReason, EmptyTable, ErrorKind, FilterOption, Group, PickerColumn,
    Row, RowActions, SESSION_COOKIE, bell, body_bytes, body_string, column_picker, columns,
    confirms_first, cookie_header, empty_table, field_error, field_error_message, filter_options,
    find_href_with, form_body, groups, input_value, multipart_body, nav_badge, notification,
    response_cookies, row_actions, row_keys, rows, session_cookie_value, set_cookie_header,
};
use tablo_core::{DeclarationError, MountError, Panel, PanelUser, RouterBuilderPanelExt};
use topcoat::router::{Body, Router, RouterBuilderDiscoverExt};
pub use two_factor::{
    authenticator_code, authenticator_code_at, now, recovery_codes, two_factor_secret,
    two_factor_state,
};

/// `panel` mounted on a router holding `db`, beside the app's own discovered pages and routes:
/// the router an app builds, without its asset bundle.
///
/// # Errors
///
/// The [`MountError`] the panel refuses to mount with, which [`refusal`] lists.
pub fn mount(db: toasty::Db, panel: Panel) -> topcoat::Result<Router> {
    Ok(Router::builder()
        .discover()
        .app_context(db)
        .panel(panel)?
        .build())
}

/// The declaration mistakes a panel refused to mount with, for a test to match on their
/// [`kind`](DeclarationError::kind).
///
/// ```rust,no_run
/// # fn refused(db: toasty::Db, panel: tablo_core::Panel) {
/// use tablo_test::{mount, refusal};
///
/// let mistakes = refusal(mount(db, panel));
/// assert!(matches!(
///     mistakes[0].kind,
///     tablo_core::DeclarationErrorKind::MissingAuthModels { .. }
/// ));
/// # }
/// ```
///
/// # Panics
///
/// When `mounted` succeeded, or failed with an error that is not a [`MountError`].
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
        self.with_cookies(&[(name.to_string(), value.to_string())])
    }

    /// Attaches every `(name, value)` pair, replacing earlier values for the
    /// same name.
    pub fn cookies(&self, cookies: &[(String, String)]) -> Self {
        self.with_cookies(cookies)
    }

    fn with_cookies(&self, cookies: &[(String, String)]) -> Self {
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

    /// Signs `user` in to the panel `Panel::new(panel)` mounts, as a successful login does, and
    /// attaches the session cookie; no password is hashed or checked.
    ///
    /// The session row goes into `db`, the router's database, keyed to [`PanelUser::user_id`]:
    /// each request loads the user back through the panel's `Authenticator::find_by_id`, so a
    /// custom authenticator works as the shipped one does.
    ///
    /// ```rust,no_run
    /// # async fn sign_in(router: &topcoat::router::Router, mut db: toasty::Db) {
    /// # use tablo_test::TestClient;
    /// let admin = tablo_core::auth::create_admin(&mut db, "admin@example.com", "secret", "Admin")
    ///     .await
    ///     .expect("seed the admin");
    /// let client = TestClient::new(router).sign_in(&db, "admin", &admin).await;
    /// # let _ = client;
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// When the session row cannot be written, e.g. the database does not register
    /// `AuthSession`.
    ///
    /// [`PanelUser::user_id`]: tablo_core::PanelUser::user_id
    pub async fn sign_in(&self, db: &toasty::Db, panel: &str, user: &impl PanelUser) -> Self {
        let token = tablo_core::auth::mint_session(&mut db.clone(), panel, user)
            .await
            .expect("record the test session");
        self.cookie(SESSION_COOKIE, &token)
    }

    /// Signs in through the login form of the panel `Panel::new(panel)` mounts, as a browser
    /// does: loads the page, posts `email` and `password` with the page's CSRF token, and keeps
    /// the cookies both responses set. Returns the client and the login response, which
    /// redirects on success and re-renders the form on a refusal.
    ///
    /// `next` is the page to land on after signing in, `""` for the panel's home. Prefer
    /// [`sign_in`](Self::sign_in) unless the test is about the login itself.
    ///
    /// # Panics
    ///
    /// When the login page renders no `csrf_token` input: the panel's authentication is off.
    pub async fn log_in(
        &self,
        panel: &str,
        email: &str,
        password: &str,
        next: &str,
    ) -> (Self, http::Response<Body>) {
        let url = format!("/{panel}/login");
        let page = self.get(&url).await;
        let cookies = response_cookies(&page);
        let html = body_string(page).await;
        let csrf = input_value(&html, "csrf_token")
            .unwrap_or_else(|| panic!("the login page embeds a csrf_token input: {html}"));
        let body = form_body(&[
            ("email", email),
            ("password", password),
            ("next", next),
            ("csrf_token", &csrf),
        ]);
        let client = self.cookies(&cookies);
        let response = client.post_form(&url, body).await;
        let session = response_cookies(&response);
        (client.cookies(&session), response)
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
        self.post(
            uri,
            http::HeaderValue::from_static("application/x-www-form-urlencoded"),
            body,
        )
        .await
    }

    /// Posts the urlencoded `body` as a panel form does: with a fresh CSRF token in both the
    /// cookie and the `csrf_token` field.
    pub async fn submit(&self, uri: &str, body: &str) -> http::Response<Body> {
        let token = Self::fresh_csrf_token();
        let field = form_body(&[("csrf_token", &token)]);
        let body = if body.is_empty() {
            field
        } else {
            format!("{body}&{field}")
        };
        self.csrf(&token).post_form(uri, body).await
    }

    /// Posts [`multipart_body`]'s `parts` as a panel form with a file input does: with a fresh
    /// CSRF token in both the cookie and a `csrf_token` part.
    pub async fn submit_multipart(
        &self,
        uri: &str,
        parts: &[(&str, Option<&str>, &str)],
    ) -> http::Response<Body> {
        const BOUNDARY: &str = "tablo-test-boundary";
        let token = Self::fresh_csrf_token();
        let mut parts = parts.to_vec();
        parts.push(("csrf_token", None, &token));
        let body = multipart_body(BOUNDARY, &parts);
        self.csrf(&token).post_multipart(uri, BOUNDARY, body).await
    }

    pub async fn post_multipart(
        &self,
        uri: &str,
        boundary: &str,
        body: String,
    ) -> http::Response<Body> {
        self.post(
            uri,
            format!("multipart/form-data; boundary={boundary}")
                .parse()
                .expect("multipart content type"),
            body,
        )
        .await
    }

    /// Calls the runtime procedure served at `path` with `args`, the JSON array of its
    /// arguments, as the browser's runtime does: a JSON `POST` that answers with the JSON of its
    /// result.
    ///
    /// ```rust,no_run
    /// # async fn read(client: &tablo_test::TestClient<'_>, id: &str) {
    /// let response = client
    ///     .call_procedure(
    ///         "/_topcoat/runtime/procedures/tablo-notifications/mark",
    ///         &format!(r#"["{id}", true]"#),
    ///     )
    ///     .await;
    /// assert_eq!(response.status(), 200);
    /// # }
    /// ```
    pub async fn call_procedure(&self, path: &str, args: &str) -> http::Response<Body> {
        self.post(
            path,
            http::HeaderValue::from_static("application/json"),
            args.to_string(),
        )
        .await
    }

    async fn post(
        &self,
        uri: &str,
        content_type: http::HeaderValue,
        body: String,
    ) -> http::Response<Body> {
        let mut request = self.request(http::Method::POST, uri);
        request.headers_mut().insert(CONTENT_TYPE, content_type);
        *request.body_mut() = Body::from(body);
        self.router.handle(request).await
    }

    fn fresh_csrf_token() -> String {
        uuid::Uuid::new_v4().to_string()
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
