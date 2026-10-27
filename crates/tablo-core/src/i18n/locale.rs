//! Which of its panel's locales a request reads.

use topcoat::{
    context::{Cx, try_context},
    l10n::{Locale, accept_language, locale, negotiate, try_locale},
};

use super::Catalog;

/// The locale of what renders outside every panel, and of a panel's English layer.
pub(crate) static ENGLISH: Locale = locale!("en");

/// The most languages of an `Accept-Language` matched, so a long header costs no more than a
/// browser's. Topcoat still parses all of them (#806).
const MAX_ACCEPTED: usize = 16;

/// `cx` as a request to the panel of `catalog`: it reads the best match of the first
/// [`MAX_ACCEPTED`] languages of its `Accept-Language` among the panel's locales, else the
/// panel's default. Entering a panel installs it, so the header is negotiated once per request.
pub(crate) fn entered(cx: &Cx, catalog: &Catalog) -> Cx {
    let offered = catalog.locales();
    let locale = match offered {
        [only] => only,
        _ => {
            negotiate(accept_language(cx).iter().take(MAX_ACCEPTED), offered).unwrap_or(&offered[0])
        }
    };
    cx.with(locale.clone())
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
