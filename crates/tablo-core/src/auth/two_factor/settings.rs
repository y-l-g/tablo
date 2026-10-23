//! The `{prefix}/two-factor` settings page, in the panel's shell: set up an authenticator or
//! mailed codes, confirm it with a code, show fresh recovery codes, and turn two-factor sign-in
//! off.

use topcoat::{
    context::Cx,
    router::{
        Body,
        error::{forbidden, not_found, see_other, too_many_requests},
        request::client_ip,
        response::response_headers,
    },
    view::BoxView,
};

use super::{
    Enrolment, FactorKind, TwoFactor, accept, accept_factor, email, enrolment,
    issue_recovery_codes,
    pages::{self, Method, Settings},
    remove_all, secret, start, start_authenticator, totp,
};
use crate::{
    auth::{
        SignedIn, infrastructure_failure,
        realm::{hosted, panel_prefix},
        session::replace_sessions,
        signed,
        token::serializable,
        unauthenticated_error,
    },
    flash::set_notification,
    outcome::Notification,
    topcoat_compat::async_page,
};

/// The form field naming what a settings POST does.
pub(super) const INTENT_FIELD: &str = "intent";

/// The form field the current password posts under, which a set-up asks for.
pub(super) const PASSWORD_FIELD: &str = "current_password";

/// What a settings POST asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Intent {
    /// Start setting up an authenticator, proven by the current password.
    Start,
    /// Start setting up mailed codes, proven by the current password: mail the first.
    StartEmail,
    /// Mail a new code, for the mailed codes being set up or on.
    Send,
    /// Confirm the authenticator being set up with one of its codes.
    Confirm,
    /// Drop the authenticator being set up.
    Cancel,
    /// Replace the recovery codes, proven by a code.
    RecoveryCodes,
    /// Turn two-factor sign-in off, proven by a code.
    Disable,
}

impl Intent {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "start" => Self::Start,
            "start-email" => Self::StartEmail,
            "send" => Self::Send,
            "confirm" => Self::Confirm,
            "cancel" => Self::Cancel,
            "recovery-codes" => Self::RecoveryCodes,
            "disable" => Self::Disable,
            _ => return None,
        })
    }

    /// Whether the intent checks a code or the password, which the throttle counts.
    fn checks_secret(self) -> bool {
        matches!(
            self,
            Self::Start | Self::StartEmail | Self::Confirm | Self::RecoveryCodes | Self::Disable
        )
    }
}

/// Where the settings page lives: `{prefix}/two-factor`.
pub(crate) fn settings_url(cx: &Cx) -> String {
    format!("{}/two-factor", panel_prefix(cx))
}

/// The signed-in user and the panel's two-factor sign-in, or the answer to a request without them.
fn configuring(cx: &Cx) -> topcoat::Result<(&SignedIn, &TwoFactor)> {
    let signed = signed(cx).ok_or_else(|| unauthenticated_error(cx))?;
    let two_factor = signed
        .realm
        .auth
        .two_factor_config()
        .ok_or_else(not_found)?;
    Ok((signed, two_factor))
}

/// The page for `enrolment` as it stands, with `refused` when a code just failed.
async fn standing<'a>(
    cx: &'a Cx,
    signed: &SignedIn,
    two_factor: &TwoFactor,
    enrolment: &Enrolment,
    refused: bool,
) -> topcoat::Result<BoxView<'a>> {
    let address = || signed.user.email().unwrap_or_default().to_string();
    let settings = match (&enrolment.confirmed, &enrolment.unconfirmed) {
        (Some(factor), _) => Settings::On {
            method: match factor.kind {
                FactorKind::EmailCode => Method::Email(address()),
                _ => Method::App,
            },
            recovery_codes: enrolment.recovery_codes,
            refused,
        },
        (None, Some(factor)) if factor.kind == FactorKind::EmailCode => Settings::Email {
            address: address(),
            refused,
        },
        (None, Some(factor)) => match secret(cx, factor)? {
            Some(secret) => setup(cx, signed, &secret, refused)?,
            None => off(signed, two_factor, false),
        },
        (None, None) => off(signed, two_factor, false),
    };
    pages::settings(cx, settings).await
}

/// The page without a factor, with `refused` when the password just failed: mailed codes are
/// offered when the panel mails them and the user stores an address.
fn off(signed: &SignedIn, two_factor: &TwoFactor, refused: bool) -> Settings {
    Settings::Off {
        email: two_factor.mails() && signed.user.email().is_some(),
        refused,
    }
}

/// The set-up step for `secret`: its QR code and its base32 text.
fn setup(cx: &Cx, signed: &SignedIn, secret: &[u8], refused: bool) -> topcoat::Result<Settings> {
    let account = signed
        .user
        .email()
        .unwrap_or_else(|| signed.user.display_name())
        .to_string();
    Ok(Settings::App {
        uri: totp::uri(secret, &hosted(cx)?.brand_name(cx), &account),
        secret: totp::encode(secret),
        refused,
    })
}

/// Keeps the response out of every cache: a set-up shows the secret, and a confirmation the
/// recovery codes.
fn no_store(cx: &Cx) {
    response_headers(cx).append(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-store"),
    );
}

/// `GET {prefix}/two-factor` — the settings, as the account's factors stand.
pub(in crate::auth) fn settings_page(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let (signed, two_factor) = configuring(cx)?;
        no_store(cx);
        crate::csrf::ensure_token(cx);
        let mut db = crate::db::db(cx);
        let enrolment = enrolment(cx, &mut db, &signed.user.user_id()).await?;
        standing(cx, signed, two_factor, &enrolment, false).await
    })
}

/// `POST {prefix}/two-factor` — do what the form's intent asks.
pub(in crate::auth) fn settings_post(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let (signed, two_factor) = configuring(cx)?;
        no_store(cx);
        let form = crate::auth::input::read_form(cx, body).await?;
        crate::csrf::verify(cx, form.last(crate::csrf::FIELD_NAME))?;
        let intent = form
            .last(INTENT_FIELD)
            .and_then(Intent::parse)
            .ok_or_else(forbidden)?;
        let code = form.last(super::challenge::CODE_FIELD).unwrap_or_default();
        let user_id = signed.user.user_id();
        // A stolen session must not guess codes at will to turn the second factor off, nor
        // passwords to turn it on.
        if intent.checks_secret() && !two_factor.throttle.attempt(&user_id, None) {
            tracing::warn!(
                panel = %signed.realm.prefix,
                client = ?client_ip(cx),
                "two-factor settings attempt throttled"
            );
            return Err(too_many_requests(two_factor.throttle.window_secs()).into());
        }
        let authenticator = signed.realm.auth.authenticator().ok_or_else(not_found)?;
        // A stolen session must not set up a factor its thief holds and lock the owner out. The
        // authenticator reads through a connection of its own, so this runs before the
        // transaction opens.
        if matches!(intent, Intent::Start | Intent::StartEmail) {
            let password = form.last(PASSWORD_FIELD).unwrap_or_default();
            if !authenticator
                .confirm_password(cx, signed.user.as_ref(), password)
                .await
                .map_err(infrastructure_failure)?
            {
                return pages::settings(cx, off(signed, two_factor, true)).await;
            }
        }
        let mut db = crate::db::db(cx);
        let mut tx = serializable(&mut db).await?;
        let enrolment = enrolment(cx, &mut tx, &user_id).await?;
        let user_type = authenticator.user_type();
        match (intent, &enrolment.confirmed, &enrolment.unconfirmed) {
            (Intent::Start, None, _) => {
                let secret = start_authenticator(cx, &mut tx, &user_id).await?;
                tx.commit().await.map_err(infrastructure_failure)?;
                pages::settings(cx, setup(cx, signed, &secret, false)?).await
            }
            (Intent::StartEmail, None, _)
                if matches!(
                    off(signed, two_factor, false),
                    Settings::Off { email: true, .. }
                ) =>
            {
                start(cx, &mut tx, &user_id, FactorKind::EmailCode, String::new()).await?;
                tx.commit().await.map_err(infrastructure_failure)?;
                email::send(cx, two_factor, signed.user.as_ref())
                    .await?
                    .or_too_many(two_factor)?;
                Err(see_other(settings_url(cx)).into())
            }
            (Intent::Send, Some(factor), _) | (Intent::Send, None, Some(factor))
                if factor.kind == FactorKind::EmailCode =>
            {
                drop(tx);
                email::send(cx, two_factor, signed.user.as_ref())
                    .await?
                    .or_too_many(two_factor)?;
                set_notification(
                    cx,
                    Notification::success(crate::t!(cx, "tablo-two-factor-sent-toast")),
                );
                Err(see_other(settings_url(cx)).into())
            }
            (Intent::Confirm, None, Some(factor)) => {
                if !accept_factor(cx, &mut tx, factor, code).await? {
                    drop(tx);
                    return standing(cx, signed, two_factor, &enrolment, true).await;
                }
                super::AuthFactor::filter(super::AuthFactor::fields().id().eq(factor.id))
                    .update()
                    .confirmed_at(Some(jiff::Timestamp::now()))
                    .exec(&mut tx)
                    .await
                    .map_err(infrastructure_failure)?;
                let codes = issue_recovery_codes(cx, &mut tx, &user_id).await?;
                // A second factor is a credential: the sessions the password alone opened end.
                let session = topcoat::session::start(cx).await?;
                replace_sessions(cx, &mut tx, signed, user_type, &session).await?;
                tx.commit().await.map_err(infrastructure_failure)?;
                two_factor.throttle.clear(&user_id, None);
                pages::settings(cx, Settings::Codes { codes, first: true }).await
            }
            (Intent::Cancel, None, Some(_)) => {
                remove_all(cx, &mut tx, &user_id).await?;
                tx.commit().await.map_err(infrastructure_failure)?;
                Err(see_other(settings_url(cx)).into())
            }
            (Intent::RecoveryCodes, Some(_), _) => {
                if !accept(cx, &mut tx, &user_id, code).await? {
                    drop(tx);
                    return standing(cx, signed, two_factor, &enrolment, true).await;
                }
                let codes = issue_recovery_codes(cx, &mut tx, &user_id).await?;
                tx.commit().await.map_err(infrastructure_failure)?;
                two_factor.throttle.clear(&user_id, None);
                pages::settings(
                    cx,
                    Settings::Codes {
                        codes,
                        first: false,
                    },
                )
                .await
            }
            (Intent::Disable, Some(_), _) => {
                if !accept(cx, &mut tx, &user_id, code).await? {
                    drop(tx);
                    return standing(cx, signed, two_factor, &enrolment, true).await;
                }
                remove_all(cx, &mut tx, &user_id).await?;
                let session = topcoat::session::start(cx).await?;
                replace_sessions(cx, &mut tx, signed, user_type, &session).await?;
                tx.commit().await.map_err(infrastructure_failure)?;
                two_factor.throttle.clear(&user_id, None);
                set_notification(
                    cx,
                    Notification::success(crate::t!(cx, "tablo-two-factor-off-toast")),
                );
                Err(see_other(settings_url(cx)).into())
            }
            // An intent the factors as they stand do not allow, such as a second set-up racing a
            // first: the page shows them as they are.
            _ => {
                drop(tx);
                Err(see_other(settings_url(cx)).into())
            }
        }
    })
}
