//! The panel's password reset: the [`PasswordResetter`] trait that finds an account by email and
//! sets its password, the [`PasswordReset`] configuration, and the `{prefix}/forgot-password` and
//! `{prefix}/reset-password` routes.

mod pages;

use std::{any::TypeId, future::Future, time::Duration};

use topcoat::{
    context::Cx,
    router::{Body, RouteFuture, error::see_other, response::IntoResponse},
};

use self::pages::{ForgotAlert, forgot_response, invalid_link, reset_response};
use super::{
    LoginThrottle, PanelUser, infrastructure_failure,
    login::login_url,
    mail::{TOKEN_FIELD, after_response, token_from_query},
    registration::new_password_errors,
    session::{credential_panels, end_credentials},
    token::{self, REVOCATION, TokenPurpose, serializable},
};
use crate::{
    BoxFuture,
    auth::realm::{Realm, current_realm, hosted, panel_prefix},
    flash::set_notification,
    form::FieldErrors,
    outcome::Notification,
};

/// How long a reset link works: one hour, fixed.
pub const RESET_LIFETIME: Duration = Duration::from_hours(1);

/// How a panel finds the account a password reset names and sets its new password, installed
/// with [`Auth::password_reset`](super::Auth::password_reset) through a [`PasswordReset`].
///
/// The shipped [`PasswordAuth`](super::PasswordAuth) resets an [`AdminUser`](super::AdminUser);
/// an app with its own user table implements it beside its
/// [`Authenticator`](super::Authenticator), for the same user type. The link goes to the address
/// [`PanelUser::email`] answers, the one the account stores, never the one the visitor typed: a
/// lookup that matches loosely, such as a collation folding accents, would otherwise mail one
/// account's link to another address. An account without one gets no link.
///
/// ```rust
/// # use tablo_core::{PanelUser, auth::{PasswordResetter, hash_password}};
/// # use topcoat::context::Cx;
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Staff {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     #[unique] email: String,
/// #     password_hash: String,
/// #     name: String,
/// # }
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { self.id.to_string() }
/// #     fn display_name(&self) -> &str { &self.name }
/// #     fn email(&self) -> Option<&str> { Some(&self.email) }
/// # }
/// struct StaffAuth;
///
/// impl PasswordResetter for StaffAuth {
///     type User = Staff;
///
///     async fn find_by_email(&self, cx: &Cx, email: &str) -> topcoat::Result<Option<Staff>> {
///         let mut db = tablo_core::db::db(cx);
///         Ok(Staff::filter_by_email(email).first().exec(&mut db).await?)
///     }
///
///     async fn reset_password(
///         &self,
///         _cx: &Cx,
///         user_id: &str,
///         password: &str,
///         ex: &mut dyn toasty::Executor,
///     ) -> topcoat::Result<()> {
///         let Ok(id) = user_id.parse::<uuid::Uuid>() else {
///             return Ok(());
///         };
///         Staff::filter_by_id(id)
///             .update()
///             .password_hash(hash_password(password).await?)
///             .exec(ex)
///             .await?;
///         Ok(())
///     }
/// }
/// ```
pub trait PasswordResetter: Send + Sync + 'static {
    /// The account a reset names: the type the panel's [`Authenticator`](super::Authenticator)
    /// loads, which mounting checks.
    type User: PanelUser;

    /// The account `email` signs in as, or `None`. The address arrives trimmed, as the visitor
    /// typed it.
    fn find_by_email(
        &self,
        cx: &Cx,
        email: &str,
    ) -> impl Future<Output = topcoat::Result<Option<Self::User>>> + Send;

    /// Set the password of the account [`PanelUser::user_id`] `user_id` names to `password`,
    /// through the reset's transaction `ex`, which commits when it returns `Ok`. The panel's
    /// authenticator still loads the account and it may enter the panel; the password is at
    /// least [`MIN_PASSWORD_CHARS`](super::MIN_PASSWORD_CHARS) long and was typed twice.
    fn reset_password(
        &self,
        cx: &Cx,
        user_id: &str,
        password: &str,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<()>> + Send;
}

/// The account a reset link is mailed to.
pub(crate) struct Recipient {
    address: String,
    user_id: String,
    /// The account's [`locale`](PanelUser::locale), which the mail is written in.
    locale: Option<String>,
}

/// [`PasswordResetter`] with its user type erased, as the panel keeps it.
pub(crate) trait DynPasswordResetter: Send + Sync {
    /// Whom to mail a reset link to for `email`, when an account that may enter the panel and
    /// stores an address signs in as `email`.
    fn recipient<'a>(
        &'a self,
        cx: &'a Cx,
        email: &'a str,
    ) -> BoxFuture<'a, topcoat::Result<Option<Recipient>>>;
    fn reset_password<'a>(
        &'a self,
        cx: &'a Cx,
        user_id: &'a str,
        password: &'a str,
        ex: &'a mut dyn toasty::Executor,
    ) -> BoxFuture<'a, topcoat::Result<()>>;
    /// The [`TypeId`] and name of [`PasswordResetter::User`].
    fn user_type(&self) -> (TypeId, &'static str);
}

impl<R: PasswordResetter> DynPasswordResetter for R {
    fn recipient<'a>(
        &'a self,
        cx: &'a Cx,
        email: &'a str,
    ) -> BoxFuture<'a, topcoat::Result<Option<Recipient>>> {
        Box::pin(async move {
            let user = self.find_by_email(cx, email).await?;
            let Some(user) = user.filter(PanelUser::can_access_panel) else {
                return Ok(None);
            };
            let Some(email) = user.email() else {
                // The page still answers that a link is on its way: it never says who exists.
                tracing::warn!(
                    user = std::any::type_name::<R::User>(),
                    "a password reset found an account whose `PanelUser::email` is `None`"
                );
                return Ok(None);
            };
            Ok(Some(Recipient {
                address: email.to_string(),
                user_id: user.user_id(),
                locale: user.locale().map(str::to_owned),
            }))
        })
    }

    fn reset_password<'a>(
        &'a self,
        cx: &'a Cx,
        user_id: &'a str,
        password: &'a str,
        ex: &'a mut dyn toasty::Executor,
    ) -> BoxFuture<'a, topcoat::Result<()>> {
        Box::pin(PasswordResetter::reset_password(
            self, cx, user_id, password, ex,
        ))
    }

    fn user_type(&self) -> (TypeId, &'static str) {
        (TypeId::of::<R::User>(), std::any::type_name::<R::User>())
    }
}

/// A panel's password reset, installed with
/// [`Auth::password_reset`](super::Auth::password_reset): the [`PasswordResetter`], and how many
/// links one address may ask for. The links go out through the panel's
/// [`AuthMail`](super::AuthMail).
///
/// ```rust
/// use tablo_core::{Auth, AuthMail, PasswordAuth, PasswordReset};
/// use topcoat::mail::Mailbox;
///
/// # fn main() -> Result<(), topcoat::mail::AddressError> {
/// let auth = Auth::password()
///     .mail(AuthMail::new(
///         Mailbox::new("no-reply@example.com")?,
///         "https://admin.example.com",
///     ))
///     .password_reset(PasswordReset::new(PasswordAuth));
/// # let _ = auth;
/// # Ok(())
/// # }
/// ```
pub struct PasswordReset {
    resetter: Box<dyn DynPasswordResetter>,
    throttle: LoginThrottle,
}

impl PasswordReset {
    /// Resets passwords through `resetter`.
    #[must_use]
    pub fn new(resetter: impl PasswordResetter) -> Self {
        Self {
            resetter: Box::new(resetter),
            throttle: LoginThrottle::new(3, Duration::from_mins(15)),
        }
    }

    /// How many reset links one email address may request in a window, from any client;
    /// defaults to three in fifteen minutes. Every request that names an address counts, whether
    /// or not an account uses it. A limit per client belongs at the proxy.
    #[must_use]
    pub fn throttle(mut self, throttle: LoginThrottle) -> Self {
        self.throttle = throttle;
        self
    }

    pub(crate) fn resetter(&self) -> &dyn DynPasswordResetter {
        &*self.resetter
    }
}

impl std::fmt::Debug for PasswordReset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordReset")
            .field("throttle", &self.throttle)
            .finish_non_exhaustive()
    }
}

/// The key the forgot-password form posts the address under.
const EMAIL_FIELD: &str = "email";
/// The keys the reset form's password controls post under, as [`new_password_errors`] names
/// them.
const PASSWORD_FIELD: &str = "password";
const CONFIRMATION_FIELD: &str = "password_confirmation";

/// Where the panel's forgot-password page lives: `{prefix}/forgot-password`.
pub(crate) fn forgot_url(cx: &Cx) -> String {
    format!("{}/forgot-password", panel_prefix(cx))
}

/// Where the panel's reset page lives: `{prefix}/reset-password`.
pub(crate) fn reset_url(cx: &Cx) -> String {
    format!("{}/reset-password", panel_prefix(cx))
}

/// Whether the request's panel installs a [`PasswordReset`].
pub(crate) fn offered(cx: &Cx) -> bool {
    current_realm(cx).is_some_and(|panel| panel.auth.reset().is_some())
}

/// `GET {prefix}/forgot-password` — the form asking for an email address.
pub(super) fn forgot_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move { forgot_response(cx, "", &FieldErrors::new(), None).await })
}

/// `POST {prefix}/forgot-password` — mail a reset link to the account the address names, then
/// answer the same whether one does or not.
pub(super) fn forgot_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current_realm(cx).ok_or_else(topcoat::router::error::not_found)?;
        let reset = panel
            .auth
            .reset()
            .ok_or_else(topcoat::router::error::not_found)?;
        let form = crate::auth::input::read_form(cx, body).await?;
        crate::csrf::verify(cx, form.last(crate::csrf::FIELD_NAME))?;
        let email = form.last(EMAIL_FIELD).map_or("", str::trim);
        if email.is_empty() {
            let mut errors = FieldErrors::new();
            errors.add(EMAIL_FIELD, crate::text!("tablo-reset-enter-email"));
            return forgot_response(cx, email, &errors, None).await;
        }
        if !reset.throttle.attempt(email, None) {
            tracing::warn!(panel = %panel.prefix, "password reset request throttled");
            return forgot_response(cx, email, &FieldErrors::new(), Some(ForgotAlert::Throttled))
                .await;
        }
        let recipient = match reset.resetter().recipient(cx, email).await {
            Ok(recipient) => recipient,
            Err(error) => {
                let error = infrastructure_failure(error);
                if crate::error::TabloError::is_infrastructure(&error) {
                    return forgot_response(
                        cx,
                        email,
                        &FieldErrors::new(),
                        Some(ForgotAlert::Unavailable),
                    )
                    .await;
                }
                return Err(error);
            }
        };
        if let Some(recipient) = recipient {
            after_response(cx, "password reset link", move |cx, panel| async move {
                mail_link(&cx, &panel, &recipient).await
            });
        }
        set_notification(
            cx,
            Notification::success(crate::t!(cx, "tablo-reset-sent"))
                .description(crate::t!(cx, "tablo-reset-sent-description")),
        );
        see_other(login_url(cx)).into_response(cx)
    })
}

/// Records a new reset token for `recipient` on `panel`, replacing the account's earlier ones
/// there, and mails its link to the account's address, in its locale.
async fn mail_link(cx: &Cx, panel: &Realm, recipient: &Recipient) -> topcoat::Result<()> {
    let Recipient {
        address,
        user_id,
        locale,
    } = recipient;
    let cx = &crate::i18n::addressed_to(cx, locale.as_deref());
    let mail = panel
        .auth
        .auth_mail()
        .ok_or_else(topcoat::router::error::not_found)?;
    let token = token::issue_alone(
        cx,
        TokenPurpose::PasswordReset,
        user_id,
        None,
        RESET_LIFETIME,
    )
    .await?;
    let link = mail.link(&reset_url(cx), &token);
    let brand = hosted(cx)?.brand_name(cx);
    let minutes = RESET_LIFETIME.as_secs() / 60;
    let message = mail.compose(
        address,
        crate::t!(cx, "tablo-reset-mail-subject", brand = brand.clone()),
        crate::t!(
            cx,
            "tablo-reset-mail-body",
            brand = brand,
            minutes = minutes,
            link = link
        ),
    )?;
    topcoat::mail::send(cx, message).await?;
    Ok(())
}

/// The live reset token `encoded` names on the request's panel.
async fn live_token(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    encoded: &str,
) -> topcoat::Result<Option<token::AuthToken>> {
    token::live(cx, ex, TokenPurpose::PasswordReset, encoded).await
}

/// `GET {prefix}/reset-password?token=…` — the form choosing a new password, or the page saying
/// the link no longer works.
pub(super) fn reset_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let token = token_from_query(cx);
        let mut db = crate::db::db(cx);
        let live = live_token(cx, &mut db, &token).await?;
        if live.is_none() {
            return invalid_link(cx).await;
        }
        reset_response(cx, &token, &FieldErrors::new()).await
    })
}

/// `POST {prefix}/reset-password` — set the new password, end every session and reset link the
/// account holds, and send it to sign in.
pub(super) fn reset_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current_realm(cx).ok_or_else(topcoat::router::error::not_found)?;
        let reset = panel
            .auth
            .reset()
            .ok_or_else(topcoat::router::error::not_found)?;
        let form = crate::auth::input::read_form(cx, body).await?;
        crate::csrf::verify(cx, form.last(crate::csrf::FIELD_NAME))?;
        let token = form.last(TOKEN_FIELD).unwrap_or_default().to_string();
        let password = form.last(PASSWORD_FIELD).unwrap_or_default();
        let confirmation = form.last(CONFIRMATION_FIELD).unwrap_or_default();
        let mut db = crate::db::db(cx);
        let Some(row) = live_token(cx, &mut db, &token).await? else {
            return invalid_link(cx).await;
        };
        // The account is loaded before the transaction opens: the authenticator reads through a
        // connection of its own.
        if !may_enter(cx, panel, &row.user_id).await? {
            return invalid_link(cx).await;
        }
        let errors = new_password_errors(password, confirmation);
        if !errors.is_empty() {
            return reset_response(cx, &token, &errors).await;
        }
        let mut tx = serializable(&mut db).await?;
        // Read again inside the transaction: of two requests racing on one link, one commits.
        if live_token(cx, &mut tx, &token).await?.is_none() {
            return invalid_link(cx).await;
        }
        reset
            .resetter()
            .reset_password(cx, &row.user_id, password, &mut tx)
            .await
            .map_err(infrastructure_failure)?;
        let (user_type, _) = reset.resetter().user_type();
        let panels = credential_panels(cx, user_type, panel);
        end_credentials(&mut tx, &row.user_id, &panels, REVOCATION).await?;
        tx.commit().await.map_err(infrastructure_failure)?;
        set_notification(
            cx,
            Notification::success(crate::t!(cx, "tablo-reset-done"))
                .description(crate::t!(cx, "tablo-reset-done-description")),
        );
        see_other(login_url(cx)).into_response(cx)
    })
}

/// Whether the account `user_id` names still signs in to `panel` and may enter it: a link mailed
/// before the account was deactivated or deleted resets nothing.
async fn may_enter(cx: &Cx, panel: &Realm, user_id: &str) -> topcoat::Result<bool> {
    let Some(authenticator) = panel.auth.authenticator() else {
        return Ok(false);
    };
    let user = authenticator
        .find_by_id(cx, user_id)
        .await
        .map_err(infrastructure_failure)?;
    Ok(user.is_some_and(|user| user.can_access_panel()))
}

/// The mistakes in a panel's [`Auth::password_reset`](super::Auth::password_reset): a reset without
/// auth, and a resetter whose user is not the authenticator's. What its mail needs,
/// [`super::mail_declaration_errors`] checks.
pub(super) fn declaration_errors(auth: &super::Auth) -> Vec<crate::DeclarationErrorKind> {
    use crate::{DeclarationErrorKind, declaration::ResetFault};

    let Some(reset) = auth.reset.as_ref() else {
        return Vec::new();
    };
    let Some(authenticator) = auth.authenticator() else {
        return vec![DeclarationErrorKind::PasswordReset(
            ResetFault::AuthDisabled,
        )];
    };
    let (user, user_name) = reset.resetter().user_type();
    if user == authenticator.user_type() {
        return Vec::new();
    }
    vec![DeclarationErrorKind::PasswordReset(ResetFault::UserType {
        resetter: user_name,
    })]
}
