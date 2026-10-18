//! The two-factor pages: the standalone challenge a sign-in answers, and the settings page in the
//! panel's shell, with the QR code a set-up shows.

use qrcode::{Color, QrCode};
use topcoat::{
    context::Cx,
    router::response::{IntoResponse, Response},
    view::{BoxView, ViewExt, attributes, view},
};

use super::{
    challenge::{CODE_FIELD, challenge_url},
    settings::INTENT_FIELD,
};
use crate::auth::{
    login::{NEXT_FIELD, login_url},
    mail::private,
    page::{AuthPage, render, switch_link},
};

/// The challenge page: the code form posting `next` along, saying the last code was wrong when
/// `refused`, which answers 422.
pub(super) async fn challenge(cx: &Cx, next: &str, refused: bool) -> topcoat::Result<Response> {
    let csrf = crate::csrf::ensure_token(cx);
    let action = challenge_url(cx);
    let next = next.to_string();
    let login = login_url(cx);
    let form = view! {
        cx =>
        <form
            method="post"
            action=(action)
            class="flex flex-col gap-5"
            data-two-factor-challenge=""
        >
            (crate::csrf::field(cx, &csrf))
            <input type="hidden" name=(NEXT_FIELD) value=(next)>
            if refused {
                tablo_ui::alert(
                    variant: tablo_ui::AlertVariant::Destructive,
                    attrs: attributes! { role="alert" data-error="invalid" },
                    tablo_ui::alert_title(
                        "That code is not valid. Try the current one."
                    )
                )
            }
            (code_field(cx, "Authentication code", "Or one of your recovery codes."))
            tablo_ui::button(
                variant: tablo_ui::ButtonVariant::Primary,
                attrs: attributes! { type="submit" class="w-full" },
                "Verify"
            )
        </form>
    }
    .boxed();
    let page = render(
        cx,
        AuthPage {
            title: "Two-factor sign-in",
            description: "Enter the code your authenticator app shows.".to_string(),
            status: refused.then_some(http::StatusCode::UNPROCESSABLE_ENTITY),
            form,
            switch: Some(switch_link(
                cx,
                "Not you?",
                &login,
                "Sign in as someone else",
            )),
            note: None,
        },
    )
    .await?;
    private(page.single().await?.into_response(cx)?)
}

/// The labelled code input, with `help` under it.
fn code_field<'a>(cx: &'a Cx, label: &'static str, help: &'static str) -> BoxView<'a> {
    view! {
        cx =>
        tablo_ui::field(
            tablo_ui::field_label(attrs: attributes! { for="two-factor-code" }, (label))
            tablo_ui::input(
                attrs: attributes! {
                    id="two-factor-code"
                    name=(CODE_FIELD)
                    type="text"
                    required=""
                    inputmode="text"
                    autocomplete="one-time-code"
                    autofocus=""
                }
            )
            tablo_ui::field_description((help))
        )
    }
    .boxed()
}

/// What the settings page shows.
pub(super) enum Settings {
    /// No authenticator: the button that starts a set-up.
    Off,
    /// An authenticator being set up: its QR code, its secret, and the confirming form.
    Setup {
        uri: String,
        secret: String,
        refused: bool,
    },
    /// Two-factor sign-in is on: the recovery codes left, and the forms that need a code.
    On {
        recovery_codes: usize,
        refused: bool,
    },
    /// Fresh recovery codes, shown this once; `first` right after the set-up confirmed.
    Codes { codes: Vec<String>, first: bool },
}

/// The settings page in the panel's shell.
pub(super) async fn settings(cx: &Cx, settings: Settings) -> topcoat::Result<BoxView<'_>> {
    let csrf = crate::csrf::current_token(cx);
    let action = super::settings::settings_url(cx);
    let intent = |value: &'static str, label: &'static str, variant| {
        view! {
            cx =>
            tablo_ui::button(
                variant: variant,
                attrs: attributes! { type="submit" name=(INTENT_FIELD) value=(value) },
                (label)
            )
        }
        .boxed()
    };
    let refusal = |refused: bool| {
        view! {
            cx =>
            if refused {
                tablo_ui::alert(
                    variant: tablo_ui::AlertVariant::Destructive,
                    attrs: attributes! { role="alert" data-error="invalid" },
                    tablo_ui::alert_title(
                        "That code is not valid. Try the current one."
                    )
                )
            }
        }
        .boxed()
    };
    let body = match settings {
        Settings::Off => view! {
            cx =>
            <p class="text-sm text-muted-foreground">
                "Two-factor sign-in is off. Turn it on to ask for a code from an authenticator \
                 app each time you sign in."
            </p>
            <form method="post" action=(action) data-two-factor="off">
                (crate::csrf::field(cx, &csrf))
                (intent(
                    "start",
                    "Set up an authenticator app",
                    tablo_ui::ButtonVariant::Primary,
                ))
            </form>
        }
        .boxed(),
        Settings::Setup {
            uri,
            secret,
            refused,
        } => view! {
            cx =>
            <p class="text-sm text-muted-foreground">
                "Scan the code with an authenticator app, or enter the key by hand, then enter \
                 the code it shows."
            </p>
            (qr_code(cx, &uri))
            <code class="block break-all font-mono text-sm" data-two-factor-secret="">
                (secret)
            </code>
            <form
                method="post"
                action=(action)
                class="flex max-w-sm flex-col gap-4"
                data-two-factor="setup"
            >
                (crate::csrf::field(cx, &csrf))
                (refusal(refused))
                (code_field(cx, "Code", "The six digits the app shows."))
                <div class="flex items-center gap-2">
                    (intent("confirm", "Turn on", tablo_ui::ButtonVariant::Primary))
                    (intent("cancel", "Cancel", tablo_ui::ButtonVariant::Outline))
                </div>
            </form>
        }
        .boxed(),
        Settings::On {
            recovery_codes,
            refused,
        } => view! {
            cx =>
            <p class="text-sm text-muted-foreground">
                "Two-factor sign-in is on. "
                (format!("{recovery_codes} recovery codes are left."))
            </p>
            <form
                method="post"
                action=(action)
                class="flex max-w-sm flex-col gap-4"
                data-two-factor="on"
            >
                (crate::csrf::field(cx, &csrf))
                (refusal(refused))
                (code_field(cx, "Code", "A code from the app, or a recovery code."))
                <div class="flex flex-wrap items-center gap-2">
                    (intent(
                        "recovery-codes",
                        "Show new recovery codes",
                        tablo_ui::ButtonVariant::Outline,
                    ))
                    (intent("disable", "Turn off", tablo_ui::ButtonVariant::Destructive))
                </div>
            </form>
        }
        .boxed(),
        Settings::Codes { codes, first } => view! {
            cx =>
            if first {
                tablo_ui::alert(
                    variant: tablo_ui::AlertVariant::Neutral,
                    attrs: attributes! { role="status" },
                    tablo_ui::alert_title("Two-factor sign-in is on")
                )
            }
            <p class="text-sm text-muted-foreground">
                "Keep these recovery codes somewhere safe. Each signs you in once without the \
                 app, and they are not shown again."
            </p>
            <ul
                class="grid max-w-sm grid-cols-2 gap-2 font-mono text-sm"
                data-recovery-codes=""
            >
                for code in codes {
                    <li>(code)</li>
                }
            </ul>
            <a
                (crate::navigation::runtime_link(cx, &action))
                class=(tablo_ui::button_variants(
                    tablo_ui::ButtonVariant::Primary,
                    tablo_ui::ButtonSize::Md,
                ))
            >
                "Done"
            </a>
        }
        .boxed(),
    };
    Ok(view! {
        cx =>
        tablo_ui::page(
            tablo_ui::page_header(tablo_ui::page_title("Two-factor sign-in"))
            tablo_ui::page_content(<div class="flex flex-col gap-4">(body)</div>)
        )
    }
    .boxed())
}

/// `uri` as a QR code: one SVG path over the dark modules, with the four-module quiet zone the
/// standard asks for.
fn qr_code<'a>(cx: &'a Cx, uri: &str) -> BoxView<'a> {
    const QUIET: usize = 4;
    let Ok(code) = QrCode::new(uri.as_bytes()) else {
        return ().boxed();
    };
    let width = code.width();
    let mut path = String::new();
    for (index, color) in code.to_colors().into_iter().enumerate() {
        if color == Color::Dark {
            let (x, y) = (index % width + QUIET, index / width + QUIET);
            path.push_str(&format!("M{x} {y}h1v1h-1z"));
        }
    }
    let size = width + 2 * QUIET;
    let view_box = format!("0 0 {size} {size}");
    view! {
        cx =>
        <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox=(view_box)
            class="size-48 rounded-md bg-white"
            shape-rendering="crispEdges"
            role="img"
            aria-label="QR code for an authenticator app"
            data-two-factor-qr=""
        >
            <path d=(path) fill="#000"></path>
        </svg>
    }
    .boxed()
}
