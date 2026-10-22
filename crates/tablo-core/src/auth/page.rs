//! The layout the sign-in and sign-up pages share: the brand above a card holding the page's
//! heading and form, and the link to the other page under it.

use tablo_ui::{card, card_content, card_description, card_header};
use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use super::realm::hosted;

/// One auth page's parts.
pub(super) struct AuthPage<'a> {
    /// The document title and the card's heading.
    pub(super) title: &'static str,
    /// The line under the heading.
    pub(super) description: String,
    /// The response status a failed attempt sets.
    pub(super) status: Option<http::StatusCode>,
    /// The card's form.
    pub(super) form: BoxView<'a>,
    /// The line under the card linking the other page.
    pub(super) switch: Option<BoxView<'a>>,
    /// A muted note under everything, such as the panel's login hint.
    pub(super) note: Option<String>,
}

/// Render `page` as a complete document.
pub(super) async fn render<'a>(cx: &'a Cx, page: AuthPage<'a>) -> topcoat::Result<BoxView<'a>> {
    let AuthPage {
        title,
        description,
        status,
        form,
        switch,
        note,
    } = page;
    let host = hosted(cx)?;
    let brand = host.page_brand(cx);
    let body = view! {
        cx =>
        <main
            class="flex min-h-svh flex-col items-center justify-center gap-6 bg-background p-6"
        >
            if let Some(status) = status {
                (status)
            }
            (brand)
            card(
                attrs: topcoat::view::attributes! { class="w-full max-w-sm" },
                card_header(
                    <h1 class="text-xl leading-none font-semibold tracking-tight">
                        (title)
                    </h1>
                    card_description((description))
                )
                card_content((form))
            )
            if let Some(switch) = switch {
                <p class="text-center text-sm text-muted-foreground">(switch)</p>
            }
            if let Some(note) = note {
                <p
                    class="max-w-sm text-center text-xs text-muted-foreground"
                    data-login-hint=""
                >
                    (note)
                </p>
            }
        </main>
    }
    .boxed();
    host.document(cx, title.to_string(), body).await
}

/// `prompt` and a link to `href` reading `label`: the line under the card.
pub(super) fn switch_link<'a>(
    cx: &'a Cx,
    prompt: &'static str,
    href: &str,
    label: &'static str,
) -> BoxView<'a> {
    let link = crate::navigation::runtime_link(cx, href);
    view! {
        cx =>
        (prompt)
        " "
        <a
            (link)
            class="font-medium text-foreground underline underline-offset-4 hover:text-primary"
        >
            (label)
        </a>
    }
    .boxed()
}
