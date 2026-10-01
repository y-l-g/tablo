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

/// Max body the login route accepts: 64 KiB.
///
/// A credential form submits an email, a password, a `next` and a CSRF token,
/// all short strings; the panel's 10 MiB form cap is for multipart uploads,
/// which the login route never carries. 64 KiB leaves room for the fields plus
/// percent-encoding growth while keeping the one unauthenticated POST route
/// from buffering a megabyte-scale body.
pub(crate) const MAX_LOGIN_BYTES: usize = 64 * 1024;

/// Form field carrying the login identifier (the shipped default reads it as
/// an email address).
pub const LOGIN_FIELD: &str = "email";
/// Form field carrying the password.
pub const PASSWORD_FIELD: &str = "password";
/// Hidden form field carrying the validated post-login destination.
pub const NEXT_FIELD: &str = "next";

/// The one error every failed login renders, so accounts cannot be
/// enumerated and panel membership stays private (ADR-0013).
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

/// Accept only same-origin relative paths as a post-login destination
/// (ADR-0013): absolute URLs, scheme-relative `//host` targets, backslash
/// tricks, and control characters are rejected.
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

/// What a failed login attempt renders: the generic credential rejection, or
/// the sign-in outage.
///
/// Two variants and no more, each with its own copy and status, so a failed
/// attempt is always a deliberate answer: a driver failure is never rendered
/// as a credential verdict, and a credential verdict never borrows the outage
/// copy. Neither variant carries driver text.
#[derive(Debug, Clone, Copy)]
enum LoginError {
    /// Wrong password, unknown account, empty fields, or valid credentials
    /// without panel access: one 403 with one message (ADR-0013).
    Credentials,
    /// The database behind sign-in could not answer: a 503 that says so,
    /// because the user must not read an outage as a rejected password.
    Unavailable,
}

impl LoginError {
    /// The message the login page's alert carries.
    fn message(self) -> &'static str {
        match self {
            Self::Credentials => GENERIC_ERROR,
            Self::Unavailable => UNAVAILABLE_ERROR,
        }
    }

    /// The status a failed attempt answers with.
    fn status(self) -> http::StatusCode {
        match self {
            Self::Credentials => http::StatusCode::FORBIDDEN,
            Self::Unavailable => http::StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

/// The status and route a login attempt renders: a [`LoginError`] page or a
/// redirect back to `next`.
///
/// The login page is a settled view, so [`ViewExt::single`] resolves it into
/// an owned handle before the response is built — no borrowed view escapes.
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
        // Login/logout carry no file parts: the values half is all they read.
        let values = crate::panel::parse_form_body(cx, body).await?.values;
        crate::csrf::verify(cx, &values)?;
        // Keep a validated destination across a failed attempt so the retry
        // form still returns where the visitor was headed (US6).
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
                match authenticator.verify(cx, login, password).await {
                    Ok(user) => user,
                    // A driver failure is not a credential verdict: the page
                    // says sign-in is unavailable instead of rendering a
                    // rejection.
                    Err(error) => return failed(cx, error, next).await,
                }
            }
            _ => None,
        };
        // One path for every failure: wrong password, unknown account, empty
        // fields, or valid credentials without panel access (ADR-0013).
        let Some(user) = verified.filter(|user| user.can_access_panel()) else {
            return login_response(cx, Some(LoginError::Credentials), next).await;
        };
        // Rotate on login: a token this request presented cannot be replayed.
        if let Some(hash) = session::token_hash(cx).await? {
            delete_session(cx, &hash).await?;
        }
        // Bounded housekeeping: the sweep goes with the rotation. A failure is
        // logged rather than fatal — a credential that verified must not become
        // a 503 because cleanup could not run.
        if let Err(error) = sweep_expired_sessions(cx).await {
            tracing::error!(error = %error, "expired-session sweep failed");
        }
        let session = session::start(cx).await?;
        // The credentials were right but the row could not be recorded: the
        // same outage page as a failed verification.
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

/// A login attempt that failed on `error`: the outage page for a driver
/// failure, whose text goes to the log; an app-authored error keeps its
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
        // Defense in depth: the route only exists on gated panels, but it
        // re-checks so a missing layer cannot leave logout ungated. Any
        // resolved user may log out — the gate answers this route for a user
        // without panel access too, so demanding panel access here would
        // strand their session row + cookie to expiry.
        if resolved(cx).is_none() {
            return Err(unauthenticated_error(cx));
        }
        // Login/logout carry no file parts: the values half is all they read.
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
/// on the panel root: a page of the old tenant's records has nothing to show
/// under the new one.
///
/// A tenant that is not one of the user's [`tenants`](super::PanelUser::tenants)
/// answers 403. The selection is stored on the session and checked against
/// the memberships on every request, so a membership removed later stops
/// applying at once.
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

/// The standalone login document: brand and dark mode honored, CSRF hidden
/// field, one error slot carrying a [`LoginError`]'s deliberate copy, no
/// sidebar (ADR-0013).
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
