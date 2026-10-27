//! The shell's language menu: the panel's locales, and the route that stores the one picked.

use tablo_ui::{dropdown_menu_item, dropdown_menu_label, dropdown_menu_separator};
use topcoat::{
    context::{Cx, try_context},
    cookie::{Cookie, CookieJarCell, Cookies, SameSite, cookies},
    icon::icon,
    router::{Body, RouteFuture, response::IntoResponse},
    view::{BoxView, ViewExt, attributes, view},
};

use crate::{Text, t};

/// The segment the language menu posts to, under the panel's prefix.
pub(crate) const LANGUAGE_SEGMENT: &str = "locale";

/// The field naming the locale picked.
const LOCALE_FIELD: &str = "locale";

/// How long a visitor's pick outlives the session: a year, as a theme choice does.
const REMEMBERED_DAYS: i64 = 365;

/// Where the language menu posts: `{prefix}/locale`.
fn language_url(cx: &Cx) -> String {
    format!("{}/{LANGUAGE_SEGMENT}", crate::auth::panel_prefix(cx))
}

/// `POST {prefix}/locale` — read the panel in another of its locales, then land back on the page
/// the menu was opened from; a locale the panel does not offer answers 403, as a tenant outside
/// the user's memberships does.
pub(crate) fn language_post(cx: &Cx, body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let parts = crate::auth::input::read_form(cx, body).await?;
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        let picked = parts
            .last(LOCALE_FIELD)
            .and_then(|tag| crate::i18n::offered_match(cx, tag))
            .ok_or_else(topcoat::router::error::forbidden)?;
        remember(cx, &picked.to_string());
        // The menu posts the page it was opened from; a `next` from elsewhere lands on the
        // panel's home rather than off-origin.
        let target = parts
            .last(crate::auth::NEXT_FIELD)
            .and_then(crate::auth::safe_next)
            .map_or_else(|| crate::auth::panel_prefix(cx), str::to_string);
        topcoat::router::error::see_other(target).into_response(cx)
    })
}

/// Stores `tag` in the panel's locale cookie, scoped to the panel's prefix so two panels on one
/// host read in two languages. Without a cookie layer the pick is dropped and the next request
/// negotiates again.
fn remember(cx: &Cx, tag: &str) {
    if try_context::<CookieJarCell>(cx).is_none() {
        return;
    }
    let cookie = Cookie::build((crate::i18n::LOCALE_COOKIE, tag.to_string()))
        .path(crate::auth::panel_prefix(cx))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(topcoat::cookie::time::Duration::days(REMEMBERED_DAYS))
        .build();
    cookies(cx).add(cookie);
}

/// The language choice: one form per locale the panel offers, the current one checked. A panel
/// offering one locale has nothing to choose, and gets nothing.
pub(crate) fn language_choices<'a>(cx: &'a Cx, csrf: &str) -> BoxView<'a> {
    let offered = crate::i18n::offered(cx);
    if offered.len() < 2 {
        return ().boxed();
    }
    let current = topcoat::l10n::try_locale(cx).cloned();
    let action = language_url(cx);
    let here = topcoat::router::request::uri(cx)
        .path_and_query()
        .map_or_else(String::new, ToString::to_string);
    let next = crate::auth::safe_next(&here)
        .unwrap_or_default()
        .to_string();
    let choices: Vec<BoxView<'a>> = offered
        .iter()
        .map(|locale| {
            let tag = locale.to_string();
            let hook = tag.clone();
            let checked = current.as_ref() == Some(locale);
            // The locale's own `tablo-shell-language` attribute names it, falling back to the
            // tag, so an app offering a locale Tablo does not translate names it in its catalog.
            let label =
                Text::derived(format!("tablo-shell-language.{tag}"), tag.clone()).localize(cx);
            let csrf = crate::csrf::field(cx, csrf);
            let action = action.clone();
            let next = next.clone();
            view! {
                cx =>
                <form method="post" action=(action)>
                    (csrf)
                    <input type="hidden" name=(crate::auth::NEXT_FIELD) value=(next)>
                    <input type="hidden" name=(LOCALE_FIELD) value=(tag)>
                    dropdown_menu_item(
                        attrs: attributes! {
                            type="submit"
                            data-locale=(hook)
                            aria-current=(checked.then_some("true"))
                        },
                        <span class="flex-1 truncate">(label)</span>
                        if checked {
                            icon(data: tablo_ui::icons::CHECK)
                        }
                    )
                </form>
            }
            .boxed()
        })
        .collect();
    view! {
        cx =>
        dropdown_menu_separator()
        dropdown_menu_label((t!(cx, "tablo-shell-language")))
        for choice in choices {
            (choice)
        }
    }
    .boxed()
}
