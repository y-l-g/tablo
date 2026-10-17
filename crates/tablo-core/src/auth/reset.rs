//! The panel's password reset: the [`PasswordResetter`] trait that finds an account by email and
//! sets its password, the [`PasswordReset`] configuration, the [`PasswordResetToken`] rows, and
//! the `{prefix}/forgot-password` and `{prefix}/reset-password` routes.

use std::{any::TypeId, future::Future, sync::Arc, time::Duration};

use jiff::Timestamp;
use topcoat::{
    context::Cx,
    mail::{Mail, Mailbox},
    router::{
        Body, RouteFuture,
        error::see_other,
        request::uri,
        response::{IntoResponse, Response},
    },
    session::Token,
    view::{BoxView, ViewExt},
};

use super::{
    LoginThrottle, PanelUser, infrastructure_failure,
    login::login_url,
    registration::new_password_errors,
    session::{credential_panels, end_credentials, token_key},
};
use crate::{
    BoxFuture,
    form::{FieldErrors, FormValues},
    notification::{Notification, set_notification},
    panel::{
        Panel, panel_prefix,
        state::{PanelState, current},
    },
    schema::{Field, Schema, Source},
    toasty_compat::IsolationLevel,
};

/// How long a reset link works: one hour, fixed.
pub const RESET_LIFETIME: Duration = Duration::from_hours(1);

/// How a panel finds the account a password reset names and sets its new password, installed
/// with [`Auth::password_reset`](super::Auth::password_reset) through a [`PasswordReset`].
///
/// The shipped [`PasswordAuth`](super::PasswordAuth) resets an [`AdminUser`](super::AdminUser);
/// an app with its own user table implements it beside its
/// [`Authenticator`](super::Authenticator), for the same user type:
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
///     fn email<'u>(&self, staff: &'u Staff) -> &'u str {
///         &staff.email
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
///             .password_hash(hash_password(password)?)
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

    /// The address the reset link goes to: the one the account stores, never the one the
    /// visitor typed. A lookup that matches loosely, such as a collation folding accents, would
    /// otherwise mail one account's link to another address.
    fn email<'u>(&self, user: &'u Self::User) -> &'u str;

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

/// [`PasswordResetter`] with its user type erased, as the panel keeps it.
pub(crate) trait DynPasswordResetter: Send + Sync {
    /// The address to mail a reset link to for `email`, and the account's id, when an account
    /// that may enter the panel signs in as `email`.
    fn recipient<'a>(
        &'a self,
        cx: &'a Cx,
        email: &'a str,
    ) -> BoxFuture<'a, topcoat::Result<Option<(String, String)>>>;
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
    ) -> BoxFuture<'a, topcoat::Result<Option<(String, String)>>> {
        Box::pin(async move {
            let user = self.find_by_email(cx, email).await?;
            Ok(user
                .filter(PanelUser::can_access_panel)
                .map(|user| (self.email(&user).to_string(), user.user_id())))
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
/// [`Auth::password_reset`](super::Auth::password_reset): the [`PasswordResetter`], the mailbox
/// the link comes from, and the absolute URL the link starts with.
///
/// ```rust
/// use tablo_core::{Auth, PasswordAuth, PasswordReset};
/// use topcoat::mail::Mailbox;
///
/// # fn main() -> Result<(), topcoat::mail::AddressError> {
/// let reset = PasswordReset::new(
///     PasswordAuth,
///     Mailbox::named("Example", "no-reply@example.com")?,
///     "https://admin.example.com",
/// );
/// let auth = Auth::password().password_reset(reset);
/// # let _ = auth;
/// # Ok(())
/// # }
/// ```
pub struct PasswordReset {
    resetter: Box<dyn DynPasswordResetter>,
    from: Mailbox,
    base_url: String,
    throttle: LoginThrottle,
}

impl PasswordReset {
    /// Resets passwords through `resetter`, mailing links from `from` that start with
    /// `base_url`, such as `https://admin.example.com`: the scheme and host the app answers on,
    /// before the panel's prefix.
    ///
    /// The base URL is configuration, not the request's `Host` header, which the client
    /// chooses: a link built from it could point a victim's email at another host. Mounting
    /// refuses a base URL that is not an absolute `http` or `https` URL, or that carries a query
    /// or fragment.
    #[must_use]
    pub fn new(
        resetter: impl PasswordResetter,
        from: Mailbox,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            resetter: Box::new(resetter),
            from,
            base_url: base_url.into().trim_end_matches('/').to_string(),
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

    /// Whether the base URL is an absolute `http` or `https` URL without a query or fragment.
    pub(crate) fn base_url_is_absolute(&self) -> bool {
        self.base_url.parse::<http::Uri>().is_ok_and(|uri| {
            matches!(uri.scheme_str(), Some("http" | "https"))
                && uri.authority().is_some()
                && uri.query().is_none()
        }) && !self.base_url.contains('#')
    }

    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl std::fmt::Debug for PasswordReset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PasswordReset")
            .field("from", &self.from.address())
            .field("base_url", &self.base_url)
            .field("throttle", &self.throttle)
            .finish_non_exhaustive()
    }
}

/// Shipped reset-link record: the SHA-256 hash of the token the link carries, the account it
/// resets, the panel that mailed it, and its expiry. The raw token is never stored.
#[derive(Debug, Clone, toasty::Model)]
pub struct PasswordResetToken {
    /// Hex-encoded SHA-256 of the link's token.
    #[key]
    pub token_hash: String,
    /// [`PanelUser::user_id`] of the account the link resets.
    #[index]
    pub user_id: String,
    /// The prefix of the panel that mailed the link: the only panel it resets on.
    pub panel: String,
    #[index]
    pub expires_at: Timestamp,
    pub created_at: Timestamp,
}

/// The key the reset form posts the link's token under.
const TOKEN_FIELD: &str = "token";
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
    current(cx).is_some_and(|panel| panel.auth.reset().is_some())
}

/// What the forgot-password page says above its form.
#[derive(Debug, Clone, Copy)]
enum ForgotAlert {
    /// The address is past its limit; answers 429.
    Throttled,
    /// The database behind the lookup could not answer; answers 503.
    Unavailable,
}

impl ForgotAlert {
    fn message(self) -> &'static str {
        match self {
            Self::Throttled => "Too many reset requests. Try again shortly.",
            Self::Unavailable => "Password reset is unavailable right now. Try again shortly.",
        }
    }

    fn status(self) -> http::StatusCode {
        match self {
            Self::Throttled => http::StatusCode::TOO_MANY_REQUESTS,
            Self::Unavailable => http::StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

/// `GET {prefix}/forgot-password` — the form asking for an email address.
pub(crate) fn forgot_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move { forgot_response(cx, "", &FieldErrors::new(), None).await })
}

/// `POST {prefix}/forgot-password` — mail a reset link to the account the address names, then
/// answer the same whether one does or not.
pub(crate) fn forgot_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current(cx).ok_or_else(topcoat::router::error::not_found)?;
        let reset = panel
            .auth
            .reset()
            .ok_or_else(topcoat::router::error::not_found)?;
        let form = crate::panel::parse_form_body(cx, body).await?;
        crate::csrf::verify(cx, form.last(crate::csrf::FIELD_NAME))?;
        let email = form.last(EMAIL_FIELD).map_or("", str::trim);
        if email.is_empty() {
            let mut errors = FieldErrors::new();
            errors.add(EMAIL_FIELD, "Enter your email address");
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
        if let Some((address, user_id)) = recipient {
            // The link is written and mailed after the answer, and a failure only logs: waiting
            // on the database or the mail server, or answering differently, would tell the
            // visitor an account uses the address.
            let cx = cx.clone();
            let panel = Arc::clone(panel);
            tokio::spawn(async move {
                let Some(reset) = panel.auth.reset() else {
                    return;
                };
                if let Err(error) = mail_link(&cx, reset, &panel.prefix, &address, &user_id).await {
                    tracing::error!(error = %error, panel = %panel.prefix, "password reset link not sent");
                }
            });
        }
        set_notification(
            cx,
            Notification::success("Check your email").description(
                "If an account uses that address, it has a link to reset its password.",
            ),
        );
        see_other(login_url(cx)).into_response(cx)
    })
}

/// Records a new reset token for `user_id` on `panel`, replacing the account's earlier ones
/// there and dropping every expired one, and mails its link to `address`.
async fn mail_link(
    cx: &Cx,
    reset: &PasswordReset,
    panel: &str,
    address: &str,
    user_id: &str,
) -> topcoat::Result<()> {
    let token = Token::random();
    let now = Timestamp::now();
    let expires_at = Timestamp::try_from(std::time::SystemTime::now() + RESET_LIFETIME)
        .map_err(topcoat::Error::from)?;
    let mut db = crate::db::db(cx);
    let mut tx = serializable(&mut db).await?;
    PasswordResetToken::filter(
        PasswordResetToken::fields()
            .user_id()
            .eq(user_id.to_string())
            .and(PasswordResetToken::fields().panel().eq(panel.to_string()))
            .or(PasswordResetToken::fields().expires_at().le(now)),
    )
    .delete()
    .exec(&mut tx)
    .await
    .map_err(infrastructure_failure)?;
    toasty::create!(PasswordResetToken {
        token_hash: token_key(&token.hash()),
        user_id: user_id.to_string(),
        panel: panel.to_string(),
        expires_at,
        created_at: now,
    })
    .exec(&mut tx)
    .await
    .map_err(infrastructure_failure)?;
    tx.commit().await.map_err(infrastructure_failure)?;
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair(TOKEN_FIELD, &token.encode())
        .finish();
    let link = format!("{}{}?{query}", reset.base_url, reset_url(cx));
    let brand = Panel::brand_name(cx);
    let minutes = RESET_LIFETIME.as_secs() / 60;
    let mail = Mail::builder()
        .from(reset.from.clone())
        .to([Mailbox::new(address)?])
        .subject(format!("Reset your {brand} password"))
        .text(format!(
            "Someone asked to reset the password of your {brand} account.\n\n\
             Open this link to choose a new one. It works once, for the next {minutes} \
             minutes:\n\n{link}\n\n\
             If you did not ask, ignore this email: your password stays the same.\n"
        ))
        .build();
    topcoat::mail::send(cx, mail).await?;
    Ok(())
}

/// A serializable transaction on `db`: two requests racing on one link, or one account, then
/// cannot both commit.
async fn serializable(db: &mut toasty::Db) -> topcoat::Result<toasty::Transaction<'_>> {
    db.transaction_builder()
        .isolation(IsolationLevel::Serializable)
        .begin()
        .await
        .map_err(infrastructure_failure)
}

/// Deletes the reset links `user_id` holds on the panels `prefixes` names, or on every panel for
/// `None`, through `ex`.
pub(super) async fn end_links(
    ex: &mut dyn toasty::Executor,
    user_id: &str,
    prefixes: Option<Vec<String>>,
) -> topcoat::Result<()> {
    let user = PasswordResetToken::fields()
        .user_id()
        .eq(user_id.to_string());
    match prefixes {
        Some(prefixes) => PasswordResetToken::filter(
            user.and(PasswordResetToken::fields().panel().in_list(prefixes)),
        ),
        None => PasswordResetToken::filter(user),
    }
    .delete()
    .exec(ex)
    .await
    .map_err(infrastructure_failure)
}

/// The forgot-password page: 422 for a missing address, the alert's status with one, else 200.
async fn forgot_response(
    cx: &Cx,
    email: &str,
    errors: &FieldErrors,
    alert: Option<ForgotAlert>,
) -> topcoat::Result<Response> {
    let status = match alert {
        Some(alert) => Some(alert.status()),
        None => (!errors.is_empty()).then_some(http::StatusCode::UNPROCESSABLE_ENTITY),
    };
    let schema = Schema::new(crate::form::required_input(
        Field::text_input::<String>(EMAIL_FIELD)
            .email()
            .label("Email"),
    ));
    let mut values = FormValues::new();
    values.insert(EMAIL_FIELD.to_string(), email.to_string());
    let fields = schema.render(cx, Source::form(&values, errors)).await?;
    let csrf = crate::csrf::ensure_token(cx);
    let action = forgot_url(cx);
    let form = topcoat::view::view! {
        cx =>
        <form method="post" action=(action) class="flex flex-col gap-5">
            (crate::csrf::field(cx, &csrf))
            if let Some(alert) = alert {
                tablo_ui::alert(
                    variant: tablo_ui::AlertVariant::Destructive,
                    attrs: topcoat::view::attributes! { role="alert" },
                    tablo_ui::alert_title((alert.message()))
                )
            }
            (fields)
            tablo_ui::button(
                variant: tablo_ui::ButtonVariant::Primary,
                attrs: topcoat::view::attributes! { type="submit" class="w-full" },
                "Email me a reset link"
            )
        </form>
    }
    .boxed();
    let page = render(
        cx,
        "Forgot your password?",
        "Enter your email and we will send you a link to choose a new password.".to_string(),
        status,
        form,
    )
    .await?;
    page.single().await?.into_response(cx)
}

/// The live reset token `encoded` names on the request's panel, or `None` for an unknown,
/// expired, or other panel's one.
async fn live_token(
    cx: &Cx,
    ex: &mut dyn toasty::Executor,
    encoded: &str,
) -> topcoat::Result<Option<PasswordResetToken>> {
    let Ok(token) = Token::decode(encoded) else {
        return Ok(None);
    };
    let row = PasswordResetToken::filter(
        PasswordResetToken::fields()
            .token_hash()
            .eq(token_key(&token.hash())),
    )
    .first()
    .exec(ex)
    .await
    .map_err(infrastructure_failure)?;
    let prefix = panel_prefix(cx);
    Ok(row.filter(|row| row.panel == prefix && row.expires_at > Timestamp::now()))
}

/// The token the reset page's query carries.
fn token_from_query(cx: &Cx) -> String {
    uri(cx)
        .query()
        .and_then(|query| {
            form_urlencoded::parse(query.as_bytes())
                .find(|(key, _)| key == TOKEN_FIELD)
                .map(|(_, value)| value.into_owned())
        })
        .unwrap_or_default()
}

/// `GET {prefix}/reset-password?token=…` — the form choosing a new password, or the page saying
/// the link no longer works.
pub(crate) fn reset_page(cx: &Cx, _body: Body) -> RouteFuture<'_> {
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
pub(crate) fn reset_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let panel = current(cx).ok_or_else(topcoat::router::error::not_found)?;
        let reset = panel
            .auth
            .reset()
            .ok_or_else(topcoat::router::error::not_found)?;
        let form = crate::panel::parse_form_body(cx, body).await?;
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
        end_credentials(&mut tx, &row.user_id, &panels).await?;
        tx.commit().await.map_err(infrastructure_failure)?;
        set_notification(
            cx,
            Notification::success("Password reset").description("Sign in with your new password."),
        );
        see_other(login_url(cx)).into_response(cx)
    })
}

/// Whether the account `user_id` names still signs in to `panel` and may enter it: a link mailed
/// before the account was deactivated or deleted resets nothing.
async fn may_enter(cx: &Cx, panel: &PanelState, user_id: &str) -> topcoat::Result<bool> {
    let Some(authenticator) = panel.auth.authenticator() else {
        return Ok(false);
    };
    let user = authenticator
        .find_by_id(cx, user_id)
        .await
        .map_err(infrastructure_failure)?;
    Ok(user.is_some_and(|user| user.can_access_panel()))
}

/// The reset form for `token`: 422 for refused passwords, else 200. It sends no referrer, so
/// the token in the page's URL stays on it, and no cache keeps it.
async fn reset_response(cx: &Cx, token: &str, errors: &FieldErrors) -> topcoat::Result<Response> {
    let status = (!errors.is_empty()).then_some(http::StatusCode::UNPROCESSABLE_ENTITY);
    let field = crate::form::required_input;
    let schema = Schema::new((
        field(
            Field::text_input::<String>(PASSWORD_FIELD)
                .password()
                .label("New password"),
        ),
        field(
            Field::text_input::<String>(CONFIRMATION_FIELD)
                .password()
                .label("Confirm new password"),
        ),
    ));
    let values = FormValues::new();
    let fields = schema.render(cx, Source::form(&values, errors)).await?;
    let csrf = crate::csrf::ensure_token(cx);
    let action = reset_url(cx);
    let token = token.to_string();
    let form = topcoat::view::view! {
        cx =>
        <form method="post" action=(action) class="flex flex-col gap-5">
            (crate::csrf::field(cx, &csrf))
            <input type="hidden" name=(TOKEN_FIELD) value=(token)>
            (fields)
            tablo_ui::button(
                variant: tablo_ui::ButtonVariant::Primary,
                attrs: topcoat::view::attributes! { type="submit" class="w-full" },
                "Reset password"
            )
        </form>
    }
    .boxed();
    let page = render(
        cx,
        "Choose a new password",
        format!("Use at least {} characters.", super::MIN_PASSWORD_CHARS),
        status,
        form,
    )
    .await?;
    private(page.single().await?.into_response(cx)?)
}

/// The page an unknown, expired or used link lands on: 404, linking to a new request.
async fn invalid_link(cx: &Cx) -> topcoat::Result<Response> {
    let forgot = forgot_url(cx);
    let link = crate::navigation::runtime_link(cx, &forgot);
    let form = topcoat::view::view! {
        cx =>
        <div class="flex flex-col gap-5">
            tablo_ui::alert(
                variant: tablo_ui::AlertVariant::Destructive,
                attrs: topcoat::view::attributes! { role="alert" },
                tablo_ui::alert_title("This reset link is invalid or has expired.")
            )
            <a
                (link)
                class="text-center text-sm font-medium text-foreground underline underline-offset-4 hover:text-primary"
            >
                "Request a new link"
            </a>
        </div>
    }
    .boxed();
    let page = render(
        cx,
        "Reset link expired",
        "Each link works once, for one hour.".to_string(),
        Some(http::StatusCode::NOT_FOUND),
        form,
    )
    .await?;
    private(page.single().await?.into_response(cx)?)
}

/// `response` with `Referrer-Policy: no-referrer` and `Cache-Control: no-store`: a page whose
/// URL or form carries a reset token.
fn private(mut response: Response) -> topcoat::Result<Response> {
    let headers = response.headers_mut();
    headers.insert(
        http::header::REFERRER_POLICY,
        http::HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

/// A reset page: the shared auth layout, with a link back to the login page.
async fn render<'a>(
    cx: &'a Cx,
    title: &'static str,
    description: String,
    status: Option<http::StatusCode>,
    form: BoxView<'a>,
) -> topcoat::Result<BoxView<'a>> {
    let login = login_url(cx);
    super::page::render(
        cx,
        super::page::AuthPage {
            title,
            description,
            status,
            form,
            switch: Some(super::page::switch_link(
                cx,
                "Remember your password?",
                &login,
                "Sign in",
            )),
            note: None,
        },
    )
    .await
}
