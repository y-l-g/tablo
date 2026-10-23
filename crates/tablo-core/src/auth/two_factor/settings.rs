//! The `{prefix}/two-factor` settings page, in the panel's shell: set up an authenticator or
//! mailed codes, confirm it with a code, show fresh recovery codes, and turn two-factor sign-in
//! off.

use topcoat::{
    context::Cx,
    router::{
        Body,
        error::{forbidden, not_found, see_other, too_many_requests},
        request::client_ip,
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
        session::{replace_sessions, start_once},
        signed,
        token::run_serializable,
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

/// `GET {prefix}/two-factor` — the settings, as the account's factors stand.
pub(in crate::auth) fn settings_page(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let (signed, two_factor) = configuring(cx)?;
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
        let user_type = authenticator.user_type();
        let mut started = None;
        let settled = run_serializable!(cx, |tx| async {
            let enrolment = enrolment(cx, tx, &user_id).await?;
            let settled = match (intent, &enrolment.confirmed, &enrolment.unconfirmed) {
                (Intent::Start, None, _) => {
                    Settled::Setup(start_authenticator(cx, tx, &user_id).await?)
                }
                (Intent::StartEmail, None, _)
                    if matches!(
                        off(signed, two_factor, false),
                        Settings::Off { email: true, .. }
                    ) =>
                {
                    start(cx, tx, &user_id, FactorKind::EmailCode, String::new()).await?;
                    Settled::Mail { notice: false }
                }
                (Intent::Send, Some(factor), _) | (Intent::Send, None, Some(factor))
                    if factor.kind == FactorKind::EmailCode =>
                {
                    Settled::Mail { notice: true }
                }
                (Intent::Confirm, None, Some(factor)) => {
                    if !accept_factor(cx, tx, factor, code).await? {
                        return Ok(Settled::Refused(Box::new(enrolment)));
                    }
                    super::AuthFactor::filter(super::AuthFactor::fields().id().eq(factor.id))
                        .update()
                        .confirmed_at(Some(jiff::Timestamp::now()))
                        .exec(&mut *tx)
                        .await
                        .map_err(infrastructure_failure)?;
                    let codes = issue_recovery_codes(cx, tx, &user_id).await?;
                    // A second factor is a credential: the sessions the password alone opened
                    // end.
                    let session = start_once(cx, &mut started).await?;
                    replace_sessions(cx, tx, signed, user_type, &session).await?;
                    Settled::Codes { codes, first: true }
                }
                (Intent::Cancel, None, Some(_)) => {
                    remove_all(cx, tx, &user_id).await?;
                    Settled::Unchanged
                }
                (Intent::RecoveryCodes, Some(_), _) => {
                    if !accept(cx, tx, &user_id, code).await? {
                        return Ok(Settled::Refused(Box::new(enrolment)));
                    }
                    let codes = issue_recovery_codes(cx, tx, &user_id).await?;
                    Settled::Codes {
                        codes,
                        first: false,
                    }
                }
                (Intent::Disable, Some(_), _) => {
                    if !accept(cx, tx, &user_id, code).await? {
                        return Ok(Settled::Refused(Box::new(enrolment)));
                    }
                    remove_all(cx, tx, &user_id).await?;
                    let session = start_once(cx, &mut started).await?;
                    replace_sessions(cx, tx, signed, user_type, &session).await?;
                    Settled::Off
                }
                // An intent the factors as they stand do not allow, such as a second set-up
                // racing a first: the page shows them as they are.
                _ => Settled::Unchanged,
            };
            Ok(settled)
        })?;
        match settled {
            Settled::Setup(secret) => pages::settings(cx, setup(cx, signed, &secret, false)?).await,
            Settled::Mail { notice } => {
                email::send(cx, two_factor, signed.user.as_ref())
                    .await?
                    .or_too_many(two_factor)?;
                if notice {
                    set_notification(
                        cx,
                        Notification::success(crate::t!(cx, "tablo-two-factor-sent-toast")),
                    );
                }
                Err(see_other(settings_url(cx)).into())
            }
            Settled::Refused(enrolment) => standing(cx, signed, two_factor, &enrolment, true).await,
            Settled::Codes { codes, first } => {
                two_factor.throttle.clear(&user_id, None);
                pages::settings(cx, Settings::Codes { codes, first }).await
            }
            Settled::Off => {
                two_factor.throttle.clear(&user_id, None);
                set_notification(
                    cx,
                    Notification::success(crate::t!(cx, "tablo-two-factor-off-toast")),
                );
                Err(see_other(settings_url(cx)).into())
            }
            Settled::Unchanged => Err(see_other(settings_url(cx)).into()),
        }
    })
}

/// What a settings POST's transaction settled, answered once it commits: a run the database
/// aborted for a concurrent one answers nothing.
enum Settled {
    /// An authenticator is being set up with this secret.
    Setup(Vec<u8>),
    /// A code goes to the account's address, with a notice when the form asked for nothing else.
    Mail { notice: bool },
    /// The code proved nothing: the factors as they stand.
    Refused(Box<Enrolment>),
    /// Fresh recovery codes to show once.
    Codes { codes: Vec<String>, first: bool },
    /// Two-factor sign-in is off.
    Off,
    /// Nothing more to show than the factors as they stand.
    Unchanged,
}
