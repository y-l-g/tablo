//! The email verification's pages: the notice asking a signed-in user for the proof, and the page
//! an unusable link lands on.

use topcoat::{
    context::Cx,
    icon::icon,
    router::response::{IntoResponse, Response},
    view::{ViewExt, attributes, view},
};

use super::notice_url;
use crate::auth::{
    login::{login_url, logout_url},
    mail::private,
    page::{AuthPage, render},
};

/// The notice for a user whose address `email` is not verified: send a link, or sign out.
pub(super) async fn notice(cx: &Cx, email: &str) -> topcoat::Result<Response> {
    let csrf = crate::csrf::ensure_token(cx);
    let action = notice_url(cx);
    let logout = logout_url(cx);
    // A link request lands back here with a notification. Only a GET takes it: a HEAD renders
    // nothing.
    let sent = (*topcoat::router::request::method(cx) == http::Method::GET)
        .then(|| crate::flash::take_notification(cx))
        .flatten();
    let form = view! {
        cx =>
        <div class="flex flex-col gap-3">
            if let Some(sent) = sent {
                tablo_ui::alert(
                    variant: tablo_ui::AlertVariant::Neutral,
                    attrs: attributes! { role="status" },
                    tablo_ui::alert_title((sent.title))
                    if let Some(description) = sent.description {
                        tablo_ui::alert_description((description))
                    }
                )
            }
            <form method="post" action=(action) data-verification-request="">
                (crate::csrf::field(cx, &csrf))
                tablo_ui::button(
                    variant: tablo_ui::ButtonVariant::Primary,
                    attrs: attributes! { type="submit" class="w-full" },
                    "Email me a verification link"
                )
            </form>
            <form method="post" action=(logout)>
                (crate::csrf::field(cx, &csrf))
                tablo_ui::button(
                    variant: tablo_ui::ButtonVariant::Outline,
                    attrs: attributes! { type="submit" class="w-full" },
                    icon(data: tablo_ui::icons::LOG_OUT)
                    "Sign out"
                )
            </form>
        </div>
    }
    .boxed();
    let page = render(
        cx,
        AuthPage {
            title: "Verify your email",
            description: format!(
                "Open the link we mail to {email} to confirm it is yours, then come back."
            ),
            status: None,
            form,
            switch: None,
            note: None,
        },
    )
    .await?;
    page.single().await?.into_response(cx)
}

/// The page an unknown, expired, used or outdated verification or email-change link lands on:
/// 404, linking to the login page.
pub(in crate::auth) async fn invalid_link(cx: &Cx) -> topcoat::Result<Response> {
    let login = login_url(cx);
    let link = crate::navigation::runtime_link(cx, &login);
    let form = view! {
        cx =>
        <div class="flex flex-col gap-5">
            tablo_ui::alert(
                variant: tablo_ui::AlertVariant::Destructive,
                attrs: attributes! { role="alert" },
                tablo_ui::alert_title(
                    "This verification link is invalid or has expired."
                )
            )
            <a
                (link)
                class="text-center text-sm font-medium text-foreground underline underline-offset-4 hover:text-primary"
            >
                "Sign in to get a new link"
            </a>
        </div>
    }
    .boxed();
    let page = render(
        cx,
        AuthPage {
            title: "Verification link expired",
            description: "Each link works once, for one day, for the address it was mailed to."
                .to_string(),
            status: Some(http::StatusCode::NOT_FOUND),
            form,
            switch: None,
            note: None,
        },
    )
    .await?;
    private(page.single().await?.into_response(cx)?)
}
