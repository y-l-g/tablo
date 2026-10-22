//! The panel's sign-in routes: the login page and POST, logout, and the
//! tenant switch.

use topcoat::{
    context::Cx,
    router::{
        Body, RouteFuture,
        request::{client_ip, uri},
        response::IntoResponse,
    },
    session,
    view::{BoxView, ViewExt},
};

use super::{
    UNAVAILABLE_ERROR, infrastructure_failure, panel_root, resolved,
    session::{delete_session, fresh_session, record, select_tenant, sweep_expired_sessions},
    signed, unauthenticated_error,
};
use crate::auth::realm::{current_realm, hosted, panel_prefix};

/// Caps the login body at 64 KiB; the panel's 10 MiB form cap covers multipart
/// uploads the login route never carries.
pub(super) const MAX_LOGIN_BYTES: usize = 64 * 1024;

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
    crate::outcome::same_origin_path(next)
}

/// The validated `?next=` the login page embeds as a hidden field.
pub(super) fn next_from_query(cx: &Cx) -> Option<String> {
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
pub(super) fn login_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    let next = next_from_query(cx).unwrap_or_default();
    Box::pin(login_response(cx, None, next))
}

/// `POST {prefix}/login` — verify, rotate the session, redirect to `next`.
pub(super) fn login_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let parts = crate::auth::input::read_form(cx, body).await?;
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        let next = parts
            .last(NEXT_FIELD)
            .and_then(safe_next)
            .map(str::to_string)
            .or_else(|| next_from_query(cx))
            .unwrap_or_default();
        let login = parts.last(LOGIN_FIELD).map(str::trim);
        let password = parts.last(PASSWORD_FIELD);
        let panel = current_realm(cx).ok_or_else(topcoat::router::error::not_found)?;
        let client = client_ip(cx);
        let verified = match (panel.auth.authenticator(), login, password) {
            (Some(authenticator), Some(login), Some(password))
                if !login.is_empty() && !password.is_empty() =>
            {
                // A login past its limit reads as a wrong password, never reaching the
                // authenticator.
                if !panel.auth.login_throttle().attempt(login, client) {
                    tracing::warn!(
                        panel = %panel.prefix,
                        client = ?client,
                        "sign-in attempt throttled"
                    );
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
            panel.auth.login_throttle().clear(login, client);
        }
        if let Err(error) = sweep_expired_sessions(cx).await {
            tracing::error!(error = %error, "expired-session sweep failed");
        }
        if let Err(error) = super::token::sweep_expired(cx).await {
            tracing::error!(error = %error, "expired-token sweep failed");
        }
        // An account with a second factor gets a challenge, not yet a session.
        if panel.auth.two_factor_config().is_some() {
            match super::two_factor::enrolled(cx, &*user).await {
                Ok(Some(kind)) => {
                    return super::two_factor::begin_challenge(cx, &*user, kind, &next).await;
                }
                Ok(None) => {}
                Err(error) => return failed(cx, error, next).await,
            }
        }
        let recorded = match fresh_session(cx).await {
            Ok(session) => record(cx, &session, &*user, panel).await,
            Err(error) => Err(error),
        };
        if let Err(error) = recorded {
            return failed(cx, error, next).await;
        }
        let target = if next.is_empty() {
            panel_root(cx)
        } else {
            next
        };
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
pub(super) fn logout_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        // Any resolved user may log out, including one without panel access,
        // so the session row and cookie do not linger to expiry.
        let Some(signed) = resolved(cx) else {
            return Err(unauthenticated_error(cx));
        };
        let parts = crate::auth::input::read_form(cx, body).await?;
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        if let Some(hash) = session::stop(cx).await? {
            delete_session(cx, &hash).await?;
        }
        // The session's open bells check it again, and stop.
        crate::bus::bus(cx).wake(&signed.realm.prefix, &signed.user.user_id());
        let target = login_url(cx);
        topcoat::router::error::see_other(target).into_response(cx)
    })
}

/// `POST {prefix}/tenant` — act for another of the user's tenants, then land
/// on the panel root; a tenant outside the user's memberships answers 403.
pub(super) fn tenant_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let Some(signed) = signed(cx) else {
            return Err(unauthenticated_error(cx));
        };
        let parts = crate::auth::input::read_form(cx, body).await?;
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        let tenant = parts
            .last(TENANT_FIELD)
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
        // The session's open bells list the old tenant's notifications: they check it again,
        // and stop.
        crate::bus::bus(cx).wake(&signed.realm.prefix, &signed.user.user_id());
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
    let hint = current_realm(cx).and_then(|panel| panel.login_hint.clone());
    let forgot = super::password_reset::offered(cx).then(|| super::password_reset::forgot_url(cx));
    let sign_up = super::registration::offered(cx)
        .then(|| super::registration::with_next(super::registration::register_url(cx), &next));
    // A sign-up or a password reset lands here with a notification. Only a GET takes it: a HEAD
    // renders nothing, and a failed attempt's page shows the failure.
    let notice = (error.is_none() && *topcoat::router::request::method(cx) == http::Method::GET)
        .then(|| crate::flash::take_notification(cx))
        .flatten();
    let password = crate::schema::fields::password_reveal(
        cx,
        "password".to_string(),
        topcoat::view::view! {
            cx =>
            tablo_ui::input(
                attrs: topcoat::view::attributes! {
                    id="password"
                    name=(PASSWORD_FIELD)
                    type="password"
                    required=""
                    autocomplete="current-password"
                }
            )
        }
        .boxed(),
    );
    let form = topcoat::view::view! {
        cx =>
        <form method="post" action=(action) class="flex flex-col gap-5">
            (crate::csrf::field(cx, &csrf))
            <input type="hidden" name=(NEXT_FIELD) value=(next)>
            if let Some(error) = error {
                tablo_ui::alert(
                    variant: tablo_ui::AlertVariant::Destructive,
                    attrs: topcoat::view::attributes! { role="alert" },
                    tablo_ui::alert_title((error.message()))
                )
            }
            if let Some(notice) = notice {
                tablo_ui::alert(
                    variant: match notice.status {
                        crate::NotificationStatus::Error => tablo_ui::AlertVariant::Destructive,
                        _ => tablo_ui::AlertVariant::Neutral,
                    },
                    attrs: topcoat::view::attributes! { role="status" },
                    tablo_ui::alert_title((notice.title))
                    if let Some(description) = notice.description {
                        tablo_ui::alert_description((description))
                    }
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
                (password)
            )
            if let Some(forgot) = forgot {
                <a
                    (crate::navigation::runtime_link(cx, &forgot))
                    class="-mt-2 self-end text-sm text-muted-foreground underline-offset-4 hover:text-foreground hover:underline"
                >
                    "Forgot your password?"
                </a>
            }
            tablo_ui::button(
                variant: tablo_ui::ButtonVariant::Primary,
                attrs: topcoat::view::attributes! { type="submit" class="w-full" },
                "Sign in"
            )
        </form>
    }
    .boxed();
    super::page::render(
        cx,
        super::page::AuthPage {
            title: "Sign in",
            description: format!("Welcome back. Sign in to {}.", hosted(cx)?.brand_name(cx)),
            status: error.map(|error| error.status()),
            form,
            switch: sign_up.map(|href| {
                super::page::switch_link(cx, "Don't have an account?", &href, "Sign up")
            }),
            note: hint,
        },
    )
    .await
}
