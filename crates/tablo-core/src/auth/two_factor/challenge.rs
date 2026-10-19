//! The second step of signing in: the challenge a correct password hands an enrolled account,
//! carried by a short-lived [`AuthToken`](crate::auth::AuthToken) in its own cookie, and the
//! `{prefix}/two-factor-challenge` routes that trade a code for a session.

use std::time::Duration;

use topcoat::{
    context::Cx,
    cookie::{Cookie, Cookies, SameSite, cookies},
    router::{
        Body, RouteFuture,
        error::{see_other, too_many_requests},
        request::client_ip,
        response::{IntoResponse, Response},
    },
};

use super::{accept, pages};
use crate::{
    auth::{
        PanelUser, infrastructure_failure,
        login::{NEXT_FIELD, login_url, next_from_query, safe_next},
        mail::private,
        panel_root,
        registration::with_next,
        session::{fresh_session, record},
        token::{self, TokenPurpose, serializable},
    },
    notification::{Notification, set_notification},
    panel::{panel_prefix, state::current},
};

/// How long a challenge waits for its code.
const CHALLENGE_LIFETIME: Duration = Duration::from_mins(5);

/// The cookie the challenge token travels in, under the `__Host-` prefix: secure, host-only,
/// whole-site.
const COOKIE_NAME: &str = "__Host-tablo-two-factor";

/// The form field the code posts under.
pub(in crate::auth) const CODE_FIELD: &str = "code";

/// Where the challenge lives: `{prefix}/two-factor-challenge`.
pub(crate) fn challenge_url(cx: &Cx) -> String {
    format!("{}/two-factor-challenge", panel_prefix(cx))
}

/// Hands `user`, whose password just verified, a challenge instead of a session: a token in its
/// own cookie, and a redirect to the challenge page carrying `next`.
pub(in crate::auth) async fn begin(
    cx: &Cx,
    user: &dyn PanelUser,
    next: &str,
) -> topcoat::Result<Response> {
    let mut db = crate::db::db(cx);
    let challenge = token::issue(
        cx,
        &mut db,
        TokenPurpose::TwoFactorChallenge,
        &user.user_id(),
        None,
        CHALLENGE_LIFETIME,
    )
    .await?;
    cookies(cx).add(
        Cookie::build((COOKIE_NAME, challenge.encode()))
            .path("/")
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Lax)
            .max_age(topcoat::cookie::time::Duration::seconds(
                i64::try_from(CHALLENGE_LIFETIME.as_secs()).unwrap_or(i64::MAX),
            ))
            .build(),
    );
    private(see_other(with_next(challenge_url(cx), next)).into_response(cx)?)
}

/// The challenge token the request's cookie carries.
fn presented(cx: &Cx) -> String {
    cookies(cx)
        .get(COOKIE_NAME)
        .map(|cookie| cookie.value().to_string())
        .unwrap_or_default()
}

/// Drops the challenge cookie.
fn forget(cx: &Cx) {
    cookies(cx).remove(Cookie::build((COOKIE_NAME, "")).path("/").secure(true));
}

/// Back to the login page, carrying `next`, saying the challenge is over.
fn expired(cx: &Cx, next: &str) -> topcoat::Result<Response> {
    forget(cx);
    set_notification(
        cx,
        Notification::warning("Sign in again")
            .description("The sign-in waited too long for its code."),
    );
    private(see_other(with_next(login_url(cx), next)).into_response(cx)?)
}

/// `GET {prefix}/two-factor-challenge` — the code form, while a challenge lasts.
pub(in crate::auth) fn challenge_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let next = next_from_query(cx).unwrap_or_default();
        let mut db = crate::db::db(cx);
        if token::live(
            cx,
            &mut db,
            TokenPurpose::TwoFactorChallenge,
            &presented(cx),
        )
        .await?
        .is_none()
        {
            return expired(cx, &next);
        }
        pages::challenge(cx, &next, false).await
    })
}

/// `POST {prefix}/two-factor-challenge` — check the code, then sign in and land on `next`.
pub(in crate::auth) fn challenge_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current(cx).ok_or_else(topcoat::router::error::not_found)?;
        let (Some(authenticator), Some(two_factor)) =
            (panel.auth.authenticator(), panel.auth.two_factor_config())
        else {
            return Err(topcoat::router::error::not_found().into());
        };
        let form = crate::panel::parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, form.last(crate::csrf::FIELD_NAME))?;
        let next = form
            .last(NEXT_FIELD)
            .and_then(safe_next)
            .map(str::to_string)
            .unwrap_or_default();
        let code = form.last(CODE_FIELD).unwrap_or_default();
        let encoded = presented(cx);
        let mut db = crate::db::db(cx);
        let Some(row) =
            token::live(cx, &mut db, TokenPurpose::TwoFactorChallenge, &encoded).await?
        else {
            return expired(cx, &next);
        };
        if !two_factor.throttle.attempt(&row.user_id, None) {
            tracing::warn!(
                panel = %panel.prefix,
                client = ?client_ip(cx),
                "two-factor attempt throttled"
            );
            return Err(too_many_requests(two_factor.throttle.window_secs()).into());
        }
        let user = authenticator
            .find_by_id(cx, &row.user_id)
            .await
            .map_err(infrastructure_failure)?
            .filter(|user| user.can_access_panel());
        let Some(user) = user else {
            return expired(cx, &next);
        };
        let mut tx = serializable(&mut db).await?;
        // Read again inside the transaction: of two requests racing on one challenge, one commits.
        if token::live(cx, &mut tx, TokenPurpose::TwoFactorChallenge, &encoded)
            .await?
            .is_none()
        {
            return expired(cx, &next);
        }
        if !accept(cx, &mut tx, &row.user_id, code).await? {
            drop(tx);
            return pages::challenge(cx, &next, true).await;
        }
        token::spend(&mut tx, &row).await?;
        tx.commit().await.map_err(infrastructure_failure)?;
        two_factor.throttle.clear(&row.user_id, None);
        forget(cx);
        let session = fresh_session(cx).await?;
        record(cx, &session, &*user, panel).await?;
        let target = if next.is_empty() {
            panel_root(cx)
        } else {
            next
        };
        private(see_other(target).into_response(cx)?)
    })
}
