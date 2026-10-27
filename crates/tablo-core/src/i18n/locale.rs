//! Which of its panel's locales a request reads.

use topcoat::{
    context::{Cx, try_context},
    l10n::{Locale, accept_language, locale, negotiate, try_locale},
};

use super::Catalog;
use crate::cookie::request_cookie;

/// The locale of what renders outside every panel, and of a panel's English layer.
pub(crate) static ENGLISH: Locale = locale!("en");

/// The most languages of an `Accept-Language` matched, so a long header costs no more than a
/// browser's. Topcoat still parses all of them (#806).
const MAX_ACCEPTED: usize = 16;

/// `cx` as a request to the panel of `catalog`: it reads the locale its visitor picked from the
/// shell's language menu, else the best match of the first [`MAX_ACCEPTED`] languages of its
/// `Accept-Language` among the panel's locales, else the panel's default. Entering a panel
/// installs it, so the cookie and the header are read once per request.
pub(crate) fn entered(cx: &Cx, catalog: &Catalog) -> Cx {
    let offered = catalog.locales();
    if let Some(chosen) = chosen(cx, offered) {
        return cx.with_many((chosen, Chosen));
    }
    let locale = match offered {
        [only] => only,
        _ => {
            negotiate(accept_language(cx).iter().take(MAX_ACCEPTED), offered).unwrap_or(&offered[0])
        }
    };
    cx.with(locale.clone())
}

/// The cookie holding the locale the visitor picked from the shell's language menu, which the
/// panel writes under its own prefix so two panels on one host read in two languages.
///
/// Prefixed for the reason the theme cookie is: a cookie is shared by every app on a host
/// whatever its port.
pub(crate) const LOCALE_COOKIE: &str = "tablo_locale";

/// Marks a request reading the locale its visitor picked, which outranks the signed-in user's
/// [`PanelUser::locale`](crate::PanelUser::locale): the menu must answer the click it just took,
/// not the account's standing preference.
#[derive(Clone, Copy)]
pub(crate) struct Chosen;

/// Whether the request reads the locale its visitor picked.
pub(crate) fn chose_locale(cx: &Cx) -> bool {
    try_context::<Chosen>(cx).is_some()
}

/// The panel locale the request's cookie names, when the panel still offers a match: a locale
/// dropped from [`Panel::locales`](crate::Panel::locales) since the visitor picked it reads as
/// none, so a stale cookie cannot pin a panel to a language it no longer translates.
fn chosen(cx: &Cx, offered: &[Locale]) -> Option<Locale> {
    matched(&request_cookie(cx, LOCALE_COOKIE)?, offered)
}

/// The locale of `offered` that `tag` names, as `fr-CA` names `fr`.
fn matched(tag: &str, offered: &[Locale]) -> Option<Locale> {
    let picked: Locale = tag.parse().ok()?;
    negotiate([&picked], offered).cloned()
}

/// The locales the request's panel offers, the default first; none outside a panel.
pub(crate) fn offered(cx: &Cx) -> &[Locale] {
    catalog(cx).map_or(&[], Catalog::locales)
}

/// The panel locale `tag` names, when the request's panel offers a match: what the language
/// menu's post accepts.
pub(crate) fn offered_match(cx: &Cx, tag: &str) -> Option<Locale> {
    matched(tag, offered(cx))
}

/// `cx` writing to an account whose [`locale`](crate::PanelUser::locale) is `locale`, such as the
/// signed-in user or a mail's recipient: its words read the panel's best match for that locale,
/// else the request's.
pub(crate) fn addressed_to(cx: &Cx, locale: Option<&Locale>) -> Cx {
    let matched = locale
        .zip(catalog(cx))
        .and_then(|(locale, catalog)| negotiate([locale], catalog.locales()));
    match matched {
        Some(matched) => cx.with(matched.clone()),
        None => cx.clone(),
    }
}

/// The request panel's catalog, and the locale its words read: the one entering the panel or
/// signing in installed. Outside a panel, Tablo's English.
pub(crate) fn catalog_and_locale(cx: &Cx) -> (&Catalog, &Locale) {
    match catalog(cx) {
        Some(catalog) => (catalog, try_locale(cx).unwrap_or(&catalog.locales()[0])),
        None => (Catalog::english(), &ENGLISH),
    }
}

/// The request panel's catalog, through the [`CatalogSource`] the panel installs.
fn catalog(cx: &Cx) -> Option<&Catalog> {
    (try_context::<CatalogSource>(cx)?.0)(cx)
}

/// Finds the request panel's catalog; the first panel mounted on a router installs it in the app
/// context.
pub(crate) struct CatalogSource(pub(crate) for<'a> fn(&'a Cx) -> Option<&'a Catalog>);
