//! The panel's sign-in routes: the login page and POST, logout, and the
//! tenant switch.

use topcoat::{
    context::Cx,
    router::{Body, RouteFuture, request::uri, response::IntoResponse},
    session,
    view::{BoxView, ViewExt},
};

use super::{
    UNAVAILABLE_ERROR, infrastructure_failure, panel_root, resolved,
    session::{delete_session, record, select_tenant, sweep_expired_sessions},
    signed, unauthenticated_error,
};
use crate::panel::{Panel, panel_prefix, state::current};

/// Caps the login body at 64 KiB; the panel's 10 MiB form cap covers multipart
/// uploads the login route never carries.
pub(crate) const MAX_LOGIN_BYTES: usize = 64 * 1024;

/// Form field carrying the login identifier (the shipped default reads it as
/// an email address).
pub const LOGIN_FIELD: &str = "email";
/// Form field carrying the password.
pub const PASSWORD_FIELD: &str = "password";
/// Hidden form field carrying the validated post-login destination.
pub const NEXT_FIELD: &str = "next";

/// Renders for every failed login so accounts cannot be enumerated and panel
/// membership stays private.
pub(super) const GENERIC_ERROR: &str = "Invalid email or password.";

/// Form field carrying the tenant the tenant switch selects.
pub const TENANT_FIELD: &str = "tenant";

/// Where the panel's login page lives: `{prefix}/login`.
pub(super) fn login_url(cx: &Cx) -> String {
    format!("{}/login", panel_prefix(cx))
}

/// Where the shell's logout control posts: `{prefix}/logout`.
pub(crate) fn logout_url(cx: &Cx) -> String {
    format!("{}/logout", panel_prefix(cx))
}

/// The login URL with a validated `next` back to the requested page.
pub(super) fn login_url_with_next(cx: &Cx) -> String {
    let mut url = login_url(cx);
    let request = uri(cx);
    let requested = match request.query() {
        Some(query) => format!("{}?{query}", request.path()),
        None => request.path().to_string(),
    };
    if let Some(next) = safe_next(&requested) {
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        serializer.append_pair(NEXT_FIELD, next);
        url.push('?');
        url.push_str(&serializer.finish());
    }
    url
}

/// Accepts only same-origin relative paths as a post-login destination;
/// rejects absolute URLs, `//host` targets, backslashes, and control characters.
pub(crate) fn safe_next(next: &str) -> Option<&str> {
    let next = next.trim();
    if !next.starts_with('/') || next.starts_with("//") {
        return None;
    }
    if next.contains('\\') || next.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(next)
}

/// The validated `?next=` the login page embeds as a hidden field.
fn next_from_query(cx: &Cx) -> Option<String> {
    let query = uri(cx).query()?;
    form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == NEXT_FIELD)
        .map(|(_, value)| value.into_owned())
        .filter(|value| safe_next(value).is_some())
}

/// Renders a failed attempt as either a credential rejection or a sign-in
/// outage, never mixing the two.
#[derive(Debug, Clone, Copy)]
enum LoginError {
    /// Wrong password, unknown account, empty fields, or valid credentials
    /// without panel access; answers 403.
    Credentials,
    /// The database behind sign-in could not answer; answers 503.
    Unavailable,
}

impl LoginError {
    fn message(self) -> &'static str {
        match self {
            Self::Credentials => GENERIC_ERROR,
            Self::Unavailable => UNAVAILABLE_ERROR,
        }
    }

    fn status(self) -> http::StatusCode {
        match self {
            Self::Credentials => http::StatusCode::FORBIDDEN,
            Self::Unavailable => http::StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

async fn login_response(
    cx: &Cx,
    error: Option<LoginError>,
    next: String,
) -> topcoat::Result<topcoat::router::response::Response> {
    let page = render_login_page(cx, error, next).await?;
    page.single().await?.into_response(cx)
}

/// `GET {prefix}/login` — the standalone login page.
pub(crate) fn login_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    let next = next_from_query(cx).unwrap_or_default();
    Box::pin(login_response(cx, None, next))
}

/// `POST {prefix}/login` — verify, rotate the session, redirect to `next`.
pub(crate) fn login_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let values = crate::panel::parse_form_body(cx, body).await?.values;
        crate::csrf::verify(cx, &values)?;
        let next = values
            .get(NEXT_FIELD)
            .and_then(|value| safe_next(value))
            .map(str::to_string)
            .or_else(|| next_from_query(cx))
            .unwrap_or_default();
        let login = values.get(LOGIN_FIELD).map(|value| value.trim());
        let password = values.get(PASSWORD_FIELD).map(String::as_str);
        let panel = current(cx).ok_or_else(topcoat::router::error::not_found)?;
        let verified = match (panel.auth.authenticator(), login, password) {
            (Some(authenticator), Some(login), Some(password))
                if !login.is_empty() && !password.is_empty() =>
            {
                // A login past its limit reads as a wrong password, never reaching the
                // authenticator.
                if !panel.auth.login_throttle().attempt(login) {
                    return login_response(cx, Some(LoginError::Credentials), next).await;
                }
                match authenticator.verify(cx, login, password).await {
                    Ok(user) => user,
                    Err(error) => return failed(cx, error, next).await,
                }
            }
            _ => None,
        };
        // One 403 for every failure: wrong password, unknown account, empty
        // fields, or valid credentials without panel access.
        let Some(user) = verified.filter(|user| user.can_access_panel()) else {
            return login_response(cx, Some(LoginError::Credentials), next).await;
        };
        if let Some(login) = login {
            panel.auth.login_throttle().clear(login);
        }
        // Rotate on login so a presented token cannot be replayed.
        if let Some(hash) = session::token_hash(cx).await? {
            delete_session(cx, &hash).await?;
        }
        if let Err(error) = sweep_expired_sessions(cx).await {
            tracing::error!(error = %error, "expired-session sweep failed");
        }
        let session = session::start(cx).await?;
        if let Err(error) = record(cx, &session, &*user, panel).await {
            return failed(cx, error, next).await;
        }
        let target = if next.is_empty() {
            panel_root(cx)
        } else {
            next
        };
        // Success stays on the `Ok` path so `Set-Cookie` flushes
        // (upstream topcoat#126).
        topcoat::router::error::see_other(target).into_response(cx)
    })
}

/// Maps a driver failure to the outage page; an app-authored error keeps its
/// mapping.
async fn failed(
    cx: &Cx,
    error: topcoat::Error,
    next: String,
) -> topcoat::Result<topcoat::router::response::Response> {
    let error = infrastructure_failure(error);
    if crate::error::TabloError::is_infrastructure(&error) {
        return login_response(cx, Some(LoginError::Unavailable), next).await;
    }
    Err(error)
}

/// `POST {prefix}/logout` — delete the session row and clear the cookie.
pub(crate) fn logout_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        // Any resolved user may log out, including one without panel access,
        // so the session row and cookie do not linger to expiry.
        if resolved(cx).is_none() {
            return Err(unauthenticated_error(cx));
        }
        let values = crate::panel::parse_form_body(cx, body).await?.values;
        crate::csrf::verify(cx, &values)?;
        if let Some(hash) = session::stop(cx).await? {
            delete_session(cx, &hash).await?;
        }
        let target = login_url(cx);
        topcoat::router::error::see_other(target).into_response(cx)
    })
}

/// `POST {prefix}/tenant` — act for another of the user's tenants, then land
/// on the panel root; a tenant outside the user's memberships answers 403.
pub(crate) fn tenant_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let Some(signed) = signed(cx) else {
            return Err(unauthenticated_error(cx));
        };
        let values = crate::panel::parse_form_body(cx, body).await?.values;
        crate::csrf::verify(cx, &values)?;
        let tenant = values
            .get(TENANT_FIELD)
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
            .filter(|tenant| {
                signed
                    .user
                    .tenants()
                    .iter()
                    .any(|membership| membership.tenant == *tenant)
            })
            .ok_or_else(topcoat::router::error::forbidden)?;
        select_tenant(cx, tenant).await?;
        topcoat::router::error::see_other(panel_root(cx)).into_response(cx)
    })
}

/// Where the shell's tenant switcher posts: `{prefix}/tenant`.
pub(crate) fn tenant_url(cx: &Cx) -> String {
    format!("{}/tenant", panel_prefix(cx))
}

/// Renders the standalone login document with brand, CSRF field, and one error
/// slot.
async fn render_login_page<'a>(
    cx: &'a Cx,
    error: Option<LoginError>,
    next: String,
) -> topcoat::Result<BoxView<'a>> {
    let csrf = crate::csrf::ensure_token(cx);
    let action = login_url(cx);
    let brand = Panel::render_brand(cx).await?;
    let hint = current(cx).and_then(|panel| panel.login_hint.clone());
    let body = topcoat::view::view! {
        cx =>
        <div class="flex min-h-svh items-center justify-center bg-muted p-6">
            <div
                class="flex w-full max-w-sm flex-col gap-6 rounded-xl border border-border bg-card p-6 text-card-foreground shadow-sm"
            >
                if let Some(error) = error {
                    (error.status())
                }
                <div class="flex flex-col items-center gap-2">
                    (brand)
                    <h1 class="text-lg font-semibold text-foreground">"Sign in"</h1>
                </div>
                <form method="post" action=(action) class="flex flex-col gap-4">
                    (crate::csrf::field(cx, &csrf))
                    <input type="hidden" name=(NEXT_FIELD) value=(next)>
                    if let Some(error) = error {
                        tablo_ui::alert(
                            variant: tablo_ui::AlertVariant::Destructive,
                            attrs: topcoat::view::attributes! { role="alert" },
                            tablo_ui::alert_title((error.message()))
                        )
                    }
                    tablo_ui::field(
                        tablo_ui::field_label(
                            attrs: topcoat::view::attributes! { for="email" },
                            "Email or username"
                        )
                        tablo_ui::input(
                            attrs: topcoat::view::attributes! {
                                id="email"
                                name=(LOGIN_FIELD)
                                type="text"
                                required=""
                                autocomplete="username"
                                autofocus=""
                            }
                        )
                    )
                    tablo_ui::field(
                        tablo_ui::field_label(
                            attrs: topcoat::view::attributes! { for="password" },
                            "Password"
                        )
                        tablo_ui::input(
                            attrs: topcoat::view::attributes! {
                                id="password"
                                name=(PASSWORD_FIELD)
                                type="password"
                                required=""
                                autocomplete="current-password"
                            }
                        )
                    )
                    tablo_ui::button(
                        variant: tablo_ui::ButtonVariant::Primary,
                        attrs: topcoat::view::attributes! { type="submit" class="w-full" },
                        "Sign in"
                    )
                </form>
                if let Some(hint) = hint {
                    <p class="text-center text-xs text-muted-foreground">(hint)</p>
                }
            </div>
        </div>
    }
    .boxed();
    Panel::render_document(cx, "Sign in".to_string(), body).await
}
