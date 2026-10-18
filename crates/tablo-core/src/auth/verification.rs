//! The panel's email verification: the [`EmailVerifier`] trait that records an address an account
//! proved it reads, the [`EmailVerification`] configuration, and the `{prefix}/verify-email`
//! routes.

mod pages;

use std::{
    any::{Any, TypeId},
    future::Future,
    time::Duration,
};

use topcoat::{
    context::Cx,
    router::{
        Body, RouteFuture,
        error::{see_other, too_many_requests},
        response::IntoResponse,
    },
};

use self::pages::{invalid_link, notice};
use super::{
    LoginThrottle, PanelUser, infrastructure_failure,
    login::login_url,
    mail::{after_response, private, token_from_query},
    panel_root, resolved,
    session::resolve,
    token::{self, TokenPurpose, serializable},
    unauthenticated_error,
};
use crate::{
    BoxFuture,
    notification::{Notification, set_notification},
    panel::{
        Panel, panel_prefix,
        state::{PanelState, current},
    },
};

/// How long a verification link works: one day, fixed.
pub const VERIFICATION_LIFETIME: Duration = Duration::from_hours(24);

/// How a panel records that an account reads the address it stores, installed with
/// [`Auth::email_verification`](super::Auth::email_verification) through an
/// [`EmailVerification`].
///
/// The link goes to the address [`PanelUser::email`] answers. The shipped
/// [`PasswordAuth`](super::PasswordAuth) verifies an [`AdminUser`](super::AdminUser); an app with
/// its own user table implements it beside its [`Authenticator`](super::Authenticator), for the
/// same user type:
///
/// ```rust
/// # use tablo_core::{PanelUser, auth::EmailVerifier};
/// # use topcoat::context::Cx;
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Staff {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     email: String,
/// #     email_verified_at: Option<jiff::Timestamp>,
/// #     name: String,
/// # }
/// # impl PanelUser for Staff {
/// #     fn user_id(&self) -> String { self.id.to_string() }
/// #     fn display_name(&self) -> &str { &self.name }
/// #     fn email(&self) -> Option<&str> { Some(&self.email) }
/// # }
/// struct StaffAuth;
///
/// impl EmailVerifier for StaffAuth {
///     type User = Staff;
///
///     fn is_verified(&self, staff: &Staff) -> bool {
///         staff.email_verified_at.is_some()
///     }
///
///     async fn store_verified_email(
///         &self,
///         _cx: &Cx,
///         user_id: &str,
///         email: &str,
///         ex: &mut dyn toasty::Executor,
///     ) -> topcoat::Result<()> {
///         let id = user_id
///             .parse::<uuid::Uuid>()
///             .map_err(|_| topcoat::router::error::not_found())?;
///         Staff::filter_by_id(id)
///             .update()
///             .email(email)
///             .email_verified_at(Some(jiff::Timestamp::now()))
///             .exec(ex)
///             .await?;
///         Ok(())
///     }
/// }
/// ```
pub trait EmailVerifier: Send + Sync + 'static {
    /// The account a link verifies: the type the panel's [`Authenticator`](super::Authenticator)
    /// loads, which mounting checks.
    type User: PanelUser;

    /// Whether `user` proved it reads the address [`PanelUser::email`] answers. A user who has
    /// not reaches only the page that asks for the proof.
    fn is_verified(&self, user: &Self::User) -> bool;

    /// Record, through the transaction `ex`, which commits when it returns `Ok`, that the account
    /// [`PanelUser::user_id`] `user_id` names proved it reads `email`: store `email` as the
    /// address it signs in with, which an email change makes new, and mark it verified. An id
    /// that names no account is an error, not a success: the link is spent either way.
    fn store_verified_email(
        &self,
        cx: &Cx,
        user_id: &str,
        email: &str,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = topcoat::Result<()>> + Send;
}

/// [`EmailVerifier`] with its user type erased, as the panel keeps it.
pub(crate) trait DynEmailVerifier: Send + Sync {
    /// Whether `user` is verified; a user of another type is not.
    fn is_verified(&self, user: &dyn PanelUser) -> bool;
    fn store_verified_email<'a>(
        &'a self,
        cx: &'a Cx,
        user_id: &'a str,
        email: &'a str,
        ex: &'a mut dyn toasty::Executor,
    ) -> BoxFuture<'a, topcoat::Result<()>>;
    /// The [`TypeId`] and name of [`EmailVerifier::User`].
    fn user_type(&self) -> (TypeId, &'static str);
}

impl<V: EmailVerifier> DynEmailVerifier for V {
    fn is_verified(&self, user: &dyn PanelUser) -> bool {
        let user: &dyn Any = user;
        user.downcast_ref::<V::User>()
            .is_some_and(|user| EmailVerifier::is_verified(self, user))
    }

    fn store_verified_email<'a>(
        &'a self,
        cx: &'a Cx,
        user_id: &'a str,
        email: &'a str,
        ex: &'a mut dyn toasty::Executor,
    ) -> BoxFuture<'a, topcoat::Result<()>> {
        Box::pin(EmailVerifier::store_verified_email(
            self, cx, user_id, email, ex,
        ))
    }

    fn user_type(&self) -> (TypeId, &'static str) {
        (TypeId::of::<V::User>(), std::any::type_name::<V::User>())
    }
}

/// A panel's email verification, installed with
/// [`Auth::email_verification`](super::Auth::email_verification): the [`EmailVerifier`], and how
/// many links one account may ask for. The links go out through the panel's
/// [`AuthMail`](super::AuthMail).
///
/// ```rust
/// use tablo_core::{Auth, AuthMail, EmailVerification, PasswordAuth};
/// use topcoat::mail::Mailbox;
///
/// # fn main() -> Result<(), topcoat::mail::AddressError> {
/// let auth = Auth::password()
///     .mail(AuthMail::new(
///         Mailbox::new("no-reply@example.com")?,
///         "https://admin.example.com",
///     ))
///     .registration(PasswordAuth)
///     .email_verification(EmailVerification::new(PasswordAuth));
/// # let _ = auth;
/// # Ok(())
/// # }
/// ```
pub struct EmailVerification {
    verifier: Box<dyn DynEmailVerifier>,
    throttle: LoginThrottle,
}

impl EmailVerification {
    /// Verifies addresses through `verifier`.
    #[must_use]
    pub fn new(verifier: impl EmailVerifier) -> Self {
        Self {
            verifier: Box::new(verifier),
            throttle: LoginThrottle::new(3, Duration::from_mins(15)),
        }
    }

    /// How many links one signed-in account may ask for in a window, from any client; defaults
    /// to three in fifteen minutes. The link a sign-up mails does not count.
    #[must_use]
    pub fn throttle(mut self, throttle: LoginThrottle) -> Self {
        self.throttle = throttle;
        self
    }

    pub(crate) fn verifier(&self) -> &dyn DynEmailVerifier {
        &*self.verifier
    }
}

impl std::fmt::Debug for EmailVerification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailVerification")
            .field("throttle", &self.throttle)
            .finish_non_exhaustive()
    }
}

/// Where the panel asks a signed-in user to verify their address: `{prefix}/verify-email`.
pub(crate) fn notice_url(cx: &Cx) -> String {
    format!("{}/verify-email", panel_prefix(cx))
}

/// Where a verification link opens: `{prefix}/verify-email/confirm`.
pub(crate) fn confirm_url(cx: &Cx) -> String {
    format!("{}/verify-email/confirm", panel_prefix(cx))
}

/// Whether `panel` makes `user` verify their address before anything else: a user without one has
/// nothing to prove, and no link could reach them.
pub(crate) fn pending(panel: &PanelState, user: &dyn PanelUser) -> bool {
    user.email().is_some()
        && panel
            .auth
            .verification()
            .is_some_and(|verification| !verification.verifier().is_verified(user))
}

/// Mails a verification link to `user` after the response, when the request's panel verifies
/// addresses and `user` stores one it has not verified: what a sign-up does.
pub(super) fn mail_after_sign_up(cx: &Cx, user: &dyn PanelUser) {
    let Some(panel) = current(cx) else {
        return;
    };
    let Some(email) = user.email().filter(|_| pending(panel, user)) else {
        return;
    };
    let (email, user_id) = (email.to_string(), user.user_id());
    after_response(cx, "verification link", move |cx, panel| async move {
        mail_link(&cx, &panel, &user_id, &email).await
    });
}

/// Records a new verification token for `user_id` proving `email`, replacing the account's
/// earlier ones on `panel`, and mails its link to `email`.
async fn mail_link(cx: &Cx, panel: &PanelState, user_id: &str, email: &str) -> topcoat::Result<()> {
    let mail = panel
        .auth
        .auth_mail()
        .ok_or_else(topcoat::router::error::not_found)?;
    let mut db = crate::db::db(cx);
    let mut tx = serializable(&mut db).await?;
    let token = token::issue(
        cx,
        &mut tx,
        TokenPurpose::EmailVerification,
        user_id,
        Some(email.to_string()),
        VERIFICATION_LIFETIME,
    )
    .await?;
    tx.commit().await.map_err(infrastructure_failure)?;
    let link = mail.link(&confirm_url(cx), &token);
    let brand = Panel::brand_name(cx);
    let message = mail.compose(
        email,
        format!("Verify your {brand} email address"),
        format!(
            "Open this link to confirm that {email} is the address of your {brand} account. It \
             works once, for the next 24 hours:\n\n{link}\n\n\
             If you did not create the account, ignore this email.\n"
        ),
    )?;
    topcoat::mail::send(cx, message).await?;
    Ok(())
}

/// `GET {prefix}/verify-email` — the page asking a signed-in user to verify their address, or
/// the panel for one who has.
pub(super) fn notice_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let signed = resolved(cx).ok_or_else(|| unauthenticated_error(cx))?;
        if !pending(&signed.panel, signed.user.as_ref()) {
            return see_other(panel_root(cx)).into_response(cx);
        }
        notice(cx, signed.user.email().unwrap_or_default()).await
    })
}

/// `POST {prefix}/verify-email` — mail the signed-in user a new link, then land back on the
/// notice.
pub(super) fn notice_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let signed = resolved(cx).ok_or_else(|| unauthenticated_error(cx))?;
        let verification = signed
            .panel
            .auth
            .verification()
            .ok_or_else(topcoat::router::error::not_found)?;
        let form = crate::panel::parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, form.last(crate::csrf::FIELD_NAME))?;
        if !pending(&signed.panel, signed.user.as_ref()) {
            return see_other(panel_root(cx)).into_response(cx);
        }
        let user_id = signed.user.user_id();
        if !verification.throttle.attempt(&user_id, None) {
            tracing::warn!(panel = %signed.panel.prefix, "verification link request throttled");
            return Err(too_many_requests(verification.throttle.window_secs()).into());
        }
        if let Some(email) = signed.user.email() {
            let email = email.to_string();
            after_response(cx, "verification link", move |cx, panel| async move {
                mail_link(&cx, &panel, &user_id, &email).await
            });
        }
        set_notification(
            cx,
            Notification::success("Check your email")
                .description("A new verification link is on its way."),
        );
        see_other(notice_url(cx)).into_response(cx)
    })
}

/// `GET {prefix}/verify-email/confirm?token=…` — verify the address the link was mailed to, then
/// land on the panel, or on the login page for a browser that is not signed in as the account.
///
/// The link verifies on `GET`, so a mail scanner that opens it verifies the address too: the
/// scanner fetched it from the mailbox the link proves.
pub(super) fn confirm_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current(cx).ok_or_else(topcoat::router::error::not_found)?;
        let verification = panel
            .auth
            .verification()
            .ok_or_else(topcoat::router::error::not_found)?;
        let encoded = token_from_query(cx);
        let mut db = crate::db::db(cx);
        let Some(row) = token::live(cx, &mut db, TokenPurpose::EmailVerification, &encoded).await?
        else {
            return invalid_link(cx).await;
        };
        // A link to an address the account no longer stores proves nothing about the one it
        // stores now.
        if !stores(cx, panel, &row).await? {
            return invalid_link(cx).await;
        }
        let email = row.email.clone().unwrap_or_default();
        let mut tx = serializable(&mut db).await?;
        // Read again inside the transaction: of two requests racing on one link, one commits.
        if token::live(cx, &mut tx, TokenPurpose::EmailVerification, &encoded)
            .await?
            .is_none()
        {
            return invalid_link(cx).await;
        }
        verification
            .verifier()
            .store_verified_email(cx, &row.user_id, &email, &mut tx)
            .await
            .map_err(infrastructure_failure)?;
        token::spend(&mut tx, &row).await?;
        tx.commit().await.map_err(infrastructure_failure)?;
        set_notification(cx, Notification::success("Email verified"));
        // The route answers signed out, so the gate resolved no session for it.
        let signed_in_here = match panel.auth.authenticator() {
            Some(authenticator) => resolve(cx, panel, authenticator)
                .await?
                .is_some_and(|signed| signed.user.user_id() == row.user_id),
            None => false,
        };
        let target = if signed_in_here {
            panel_root(cx)
        } else {
            login_url(cx)
        };
        private(see_other(target).into_response(cx)?)
    })
}

/// Whether the account `row` names still signs in to `panel`, may enter it, and stores the
/// address `row` proves.
async fn stores(cx: &Cx, panel: &PanelState, row: &token::AuthToken) -> topcoat::Result<bool> {
    let Some(authenticator) = panel.auth.authenticator() else {
        return Ok(false);
    };
    let user = authenticator
        .find_by_id(cx, &row.user_id)
        .await
        .map_err(infrastructure_failure)?;
    Ok(user.is_some_and(|user| {
        user.can_access_panel() && user.email().is_some() && user.email() == row.email.as_deref()
    }))
}

/// The mistakes in a panel's [`Auth::email_verification`](super::Auth::email_verification): a
/// verification without auth, and a verifier whose user is not the authenticator's.
pub(super) fn declaration_errors(auth: &super::Auth) -> Vec<crate::DeclarationErrorKind> {
    use crate::{DeclarationErrorKind, declaration::VerificationFault};

    let Some(verification) = auth.verification.as_ref() else {
        return Vec::new();
    };
    let Some(authenticator) = auth.authenticator() else {
        return vec![DeclarationErrorKind::EmailVerification(
            VerificationFault::AuthDisabled,
        )];
    };
    let (user, user_name) = verification.verifier().user_type();
    if user == authenticator.user_type() {
        return Vec::new();
    }
    vec![DeclarationErrorKind::EmailVerification(
        VerificationFault::UserType {
            verifier: user_name,
        },
    )]
}
