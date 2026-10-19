//! The password reset's pages: the forgot-password form, the new-password form, and the page an
//! unusable link lands on.

use topcoat::{
    context::Cx,
    router::response::{IntoResponse, Response},
    view::{BoxView, ViewExt},
};

use super::{CONFIRMATION_FIELD, EMAIL_FIELD, PASSWORD_FIELD, forgot_url, reset_url};
use crate::{
    auth::{
        login::login_url,
        mail::{TOKEN_FIELD, private},
    },
    form::{FieldErrors, FormValues},
    schema::{Field, Schema, Source},
};

/// What the forgot-password page says above its form.
#[derive(Debug, Clone, Copy)]
pub(super) enum ForgotAlert {
    /// The address is past its limit; answers 429.
    Throttled,
    /// The database behind the lookup could not answer; answers 503.
    Unavailable,
}

impl ForgotAlert {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Throttled => "Too many reset requests. Try again shortly.",
            Self::Unavailable => "Password reset is unavailable right now. Try again shortly.",
        }
    }

    pub(super) fn status(self) -> http::StatusCode {
        match self {
            Self::Throttled => http::StatusCode::TOO_MANY_REQUESTS,
            Self::Unavailable => http::StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

/// The forgot-password page: 422 for a missing address, the alert's status with one, else 200.
pub(super) async fn forgot_response(
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

/// The reset form for `token`: 422 for refused passwords, else 200. It sends no referrer, so
/// the token in the page's URL stays on it, and no cache keeps it.
pub(super) async fn reset_response(
    cx: &Cx,
    token: &str,
    errors: &FieldErrors,
) -> topcoat::Result<Response> {
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
        format!(
            "Use at least {} characters.",
            crate::auth::MIN_PASSWORD_CHARS
        ),
        status,
        form,
    )
    .await?;
    private(page.single().await?.into_response(cx)?)
}

/// The page an unknown, expired or used link lands on: 404, linking to a new request.
pub(super) async fn invalid_link(cx: &Cx) -> topcoat::Result<Response> {
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

/// A reset page: the shared auth layout, with a link back to the login page.
async fn render<'a>(
    cx: &'a Cx,
    title: &'static str,
    description: String,
    status: Option<http::StatusCode>,
    form: BoxView<'a>,
) -> topcoat::Result<BoxView<'a>> {
    let login = login_url(cx);
    crate::auth::page::render(
        cx,
        crate::auth::page::AuthPage {
            title,
            description,
            status,
            form,
            switch: Some(crate::auth::page::switch_link(
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
