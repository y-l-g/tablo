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
    settings::{INTENT_FIELD, PASSWORD_FIELD},
};
use crate::{
    auth::{
        login::{NEXT_FIELD, login_url},
        mail::private,
        page::{AuthPage, render, switch_link},
    },
    t,
};

/// What the challenge page says above its form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Notice {
    None,
    /// The last code was wrong; the page answers 422.
    Refused,
    /// A new code is mailed.
    Sent,
}

/// The challenge page: the code form posting `next` along, with the button that mails a new code
/// when the account's codes are `mailed`.
pub(super) async fn challenge(
    cx: &Cx,
    next: &str,
    mailed: bool,
    notice: Notice,
) -> topcoat::Result<Response> {
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
            if notice == Notice::Refused {
                tablo_ui::alert(
                    variant: tablo_ui::AlertVariant::Destructive,
                    attrs: attributes! { role="alert" data-error="invalid" },
                    tablo_ui::alert_title((t!(cx, "tablo-two-factor-invalid")))
                )
            }
            if notice == Notice::Sent {
                tablo_ui::alert(
                    variant: tablo_ui::AlertVariant::Neutral,
                    attrs: attributes! { role="status" data-two-factor-sent="" },
                    tablo_ui::alert_title((t!(cx, "tablo-two-factor-sent")))
                )
            }
            (code_field(cx, "tablo-two-factor-code-label", "tablo-two-factor-code-help"))
            tablo_ui::button(
                variant: tablo_ui::ButtonVariant::Primary,
                attrs: attributes! { type="submit" class="w-full" },
                (t!(cx, "tablo-two-factor-verify"))
            )
            if mailed {
                tablo_ui::button(
                    variant: tablo_ui::ButtonVariant::Outline,
                    attrs: attributes! {
                        type="submit"
                        name=(INTENT_FIELD)
                        value="send"
                        formnovalidate=""
                        class="w-full"
                    },
                    (t!(cx, "tablo-two-factor-resend"))
                )
            }
        </form>
    }
    .boxed();
    let description = if mailed {
        t!(cx, "tablo-two-factor-mailed-description")
    } else {
        t!(cx, "tablo-two-factor-app-description")
    };
    let page = render(
        cx,
        AuthPage {
            title: t!(cx, "tablo-shell-two-factor"),
            description,
            status: (notice == Notice::Refused).then_some(http::StatusCode::UNPROCESSABLE_ENTITY),
            form,
            switch: Some(switch_link(
                cx,
                "tablo-two-factor-not-you",
                &login,
                "tablo-two-factor-switch",
            )),
            note: None,
        },
    )
    .await?;
    private(page.single().await?.into_response(cx)?)
}

/// The code input labelled by the message `label`, with the message `help` under it.
fn code_field<'a>(cx: &'a Cx, label: &'static str, help: &'static str) -> BoxView<'a> {
    let (label, help) = (t!(cx, label), t!(cx, help));
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

/// The current password a set-up asks for.
fn password_field(cx: &Cx) -> BoxView<'_> {
    view! {
        cx =>
        tablo_ui::field(
            tablo_ui::field_label(
                attrs: attributes! { for="two-factor-password" },
                (t!(cx, "tablo-auth-current-password"))
            )
            tablo_ui::input(
                attrs: attributes! {
                    id="two-factor-password"
                    name=(PASSWORD_FIELD)
                    type="password"
                    required=""
                    autocomplete="current-password"
                }
            )
            tablo_ui::field_description((t!(cx, "tablo-two-factor-password-help")))
        )
    }
    .boxed()
}

/// What the settings page shows.
pub(super) enum Settings {
    /// No factor: the password and the buttons that start a set-up, mailed codes among them when
    /// `email`, with `refused` when the password just failed.
    Off { email: bool, refused: bool },
    /// An authenticator being set up: its QR code, its secret, and the confirming form.
    App {
        uri: String,
        secret: String,
        refused: bool,
    },
    /// Mailed codes being set up: the confirming form for the code mailed to `address`.
    Email { address: String, refused: bool },
    /// Two-factor sign-in is on: how, the recovery codes left, and the forms that need a code.
    On {
        method: Method,
        recovery_codes: usize,
        refused: bool,
    },
    /// Fresh recovery codes, shown this once; `first` right after the set-up confirmed.
    Codes { codes: Vec<String>, first: bool },
}

/// How an account's second factor gives its codes.
pub(super) enum Method {
    /// An authenticator app.
    App,
    /// Mail to the address.
    Email(String),
}

/// The settings page in the panel's shell.
pub(super) async fn settings(cx: &Cx, settings: Settings) -> topcoat::Result<BoxView<'_>> {
    let csrf = crate::csrf::current_token(cx);
    let action = super::settings::settings_url(cx);
    let intent = move |value: &'static str, label: &'static str, variant| {
        let label = t!(cx, label);
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
    let refusal = move |refused: bool| {
        let invalid = t!(cx, "tablo-two-factor-invalid");
        view! {
            cx =>
            if refused {
                tablo_ui::alert(
                    variant: tablo_ui::AlertVariant::Destructive,
                    attrs: attributes! { role="alert" data-error="invalid" },
                    tablo_ui::alert_title((invalid))
                )
            }
        }
        .boxed()
    };
    // Mails a code; it skips the code field's `required`, which it does not read.
    let send = move || {
        view! {
            cx =>
            tablo_ui::button(
                variant: tablo_ui::ButtonVariant::Outline,
                attrs: attributes! {
                    type="submit"
                    name=(INTENT_FIELD)
                    value="send"
                    formnovalidate=""
                },
                (t!(cx, "tablo-two-factor-resend"))
            )
        }
        .boxed()
    };
    let body = match settings {
        Settings::Off { email, refused } => view! {
            cx =>
            <p class="text-sm text-muted-foreground">
                (t!(cx, "tablo-two-factor-off"))
            </p>
            <form
                method="post"
                action=(action)
                class="flex max-w-sm flex-col gap-4"
                data-two-factor="off"
            >
                (crate::csrf::field(cx, &csrf))
                if refused {
                    tablo_ui::alert(
                        variant: tablo_ui::AlertVariant::Destructive,
                        attrs: attributes! { role="alert" data-error="password" },
                        tablo_ui::alert_title(
                            (t!(cx, "tablo-two-factor-wrong-password"))
                        )
                    )
                }
                (password_field(cx))
                <div class="flex flex-wrap items-center gap-2">
                    (intent(
                        "start",
                        "tablo-two-factor-start-app",
                        tablo_ui::ButtonVariant::Primary,
                    ))
                    if email {
                        (intent(
                            "start-email",
                            "tablo-two-factor-start-email",
                            tablo_ui::ButtonVariant::Outline,
                        ))
                    }
                </div>
            </form>
        }
        .boxed(),
        Settings::Email { address, refused } => view! {
            cx =>
            <p class="text-sm text-muted-foreground">
                (t!(cx, "tablo-two-factor-email-setup", address = address))
            </p>
            <form
                method="post"
                action=(action)
                class="flex max-w-sm flex-col gap-4"
                data-two-factor="setup"
            >
                (crate::csrf::field(cx, &csrf))
                (refusal(refused))
                (code_field(cx, "tablo-two-factor-code", "tablo-two-factor-email-help"))
                <div class="flex flex-wrap items-center gap-2">
                    (intent(
                        "confirm",
                        "tablo-two-factor-turn-on",
                        tablo_ui::ButtonVariant::Primary,
                    ))
                    (send())
                    (intent("cancel", "tablo-cancel", tablo_ui::ButtonVariant::Outline))
                </div>
            </form>
        }
        .boxed(),
        Settings::App {
            uri,
            secret,
            refused,
        } => view! {
            cx =>
            <p class="text-sm text-muted-foreground">
                (t!(cx, "tablo-two-factor-scan"))
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
                (code_field(cx, "tablo-two-factor-code", "tablo-two-factor-app-help"))
                <div class="flex items-center gap-2">
                    (intent(
                        "confirm",
                        "tablo-two-factor-turn-on",
                        tablo_ui::ButtonVariant::Primary,
                    ))
                    (intent("cancel", "tablo-cancel", tablo_ui::ButtonVariant::Outline))
                </div>
            </form>
        }
        .boxed(),
        Settings::On {
            method,
            recovery_codes,
            refused,
        } => {
            let (status, help) = match &method {
                Method::App => (
                    t!(cx, "tablo-two-factor-on-app", left = recovery_codes),
                    "tablo-two-factor-on-app-help",
                ),
                Method::Email(address) => (
                    t!(
                        cx,
                        "tablo-two-factor-on-email",
                        address = address,
                        left = recovery_codes
                    ),
                    "tablo-two-factor-on-email-help",
                ),
            };
            let mailed = matches!(method, Method::Email(_));
            view! {
                cx =>
                <p class="text-sm text-muted-foreground">(status)</p>
                <form
                    method="post"
                    action=(action)
                    class="flex max-w-sm flex-col gap-4"
                    data-two-factor="on"
                >
                    (crate::csrf::field(cx, &csrf))
                    (refusal(refused))
                    (code_field(cx, "tablo-two-factor-code", help))
                    <div class="flex flex-wrap items-center gap-2">
                        if mailed {
                            (send())
                        }
                        (intent(
                            "recovery-codes",
                            "tablo-two-factor-new-codes",
                            tablo_ui::ButtonVariant::Outline,
                        ))
                        (intent(
                            "disable",
                            "tablo-two-factor-turn-off",
                            tablo_ui::ButtonVariant::Destructive,
                        ))
                    </div>
                </form>
            }
            .boxed()
        }
        Settings::Codes { codes, first } => view! {
            cx =>
            if first {
                tablo_ui::alert(
                    variant: tablo_ui::AlertVariant::Neutral,
                    attrs: attributes! { role="status" },
                    tablo_ui::alert_title((t!(cx, "tablo-two-factor-on")))
                )
            }
            <p class="text-sm text-muted-foreground">
                (t!(cx, "tablo-two-factor-keep-codes"))
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
                (t!(cx, "tablo-two-factor-done"))
            </a>
        }
        .boxed(),
    };
    Ok(view! {
        cx =>
        tablo_ui::page(
            tablo_ui::page_header(
                tablo_ui::page_title((t!(cx, "tablo-shell-two-factor")))
            )
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
            aria-label=(t!(cx, "tablo-two-factor-qr"))
            data-two-factor-qr=""
        >
            <path d=(path) fill="#000"></path>
        </svg>
    }
    .boxed()
}
