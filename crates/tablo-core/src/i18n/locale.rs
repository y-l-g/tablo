//! The request's [`Locale`]: which of its panel's locales it reads.

use std::fmt;

use topcoat::context::{Cx, try_app_context};
use unic_langid::LanguageIdentifier;

use super::Catalog;

/// A BCP 47 language tag a panel offers, such as `fr` or `pt-BR`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Locale(pub(super) LanguageIdentifier);

impl Locale {
    /// The language subtag: `pt` for `pt-BR`.
    pub fn language(&self) -> &str {
        self.0.language.as_str()
    }

    /// Parses `tag`; `None` when it is no language tag.
    pub(crate) fn parse(tag: &str) -> Option<Self> {
        tag.parse().ok().map(Self)
    }

    /// Whether a catalog for `self` serves `target`: the same tag, or `self` names the language
    /// alone, as `fr` serves `fr-CA`.
    pub(super) fn serves(&self, target: &Self) -> bool {
        self.0.matches(&target.0, true, false)
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The locale the request reads its words in: the signed-in user's
/// [`locale`](crate::PanelUser::locale), else the best match of the request's `Accept-Language`,
/// else the first of the request panel's [`locales`](crate::Panel::locales), each only when the
/// panel offers it. Outside a panel, `en`.
///
/// A panel offering one locale always reads it. A user's locale or a browser's language matches
/// an offered locale of the same tag, else of the same language: `fr-CA` reads a panel's `fr`.
pub fn locale(cx: &Cx) -> Locale {
    let (catalog, index) = catalog_and_index(cx);
    catalog.locales()[index].clone()
}

/// The request panel's catalog, and the index of the request's locale in it.
pub(crate) fn catalog_and_index(cx: &Cx) -> (&Catalog, usize) {
    let Some(source) = try_app_context::<LocaleSource>(cx) else {
        return (Catalog::english(), 0);
    };
    let Some(catalog) = (source.catalog)(cx) else {
        return (Catalog::english(), 0);
    };
    let locales = catalog.locales();
    if locales.len() == 1 {
        return (catalog, 0);
    }
    let preferred = (source.preferred)(cx)
        .and_then(|tag| Locale::parse(&tag))
        .and_then(|tag| best_match(locales, &[tag.0]));
    let index = preferred
        .or_else(|| best_match(locales, &accepted(cx)))
        .unwrap_or(0);
    (catalog, index)
}

/// The languages the request's `Accept-Language` asks for, most wanted first.
fn accepted(cx: &Cx) -> Vec<LanguageIdentifier> {
    topcoat::context::try_request_context::<http::request::Parts>(cx)
        .and_then(|parts| parts.headers.get(http::header::ACCEPT_LANGUAGE))
        .and_then(|value| value.to_str().ok())
        .map(fluent_langneg::accepted_languages::parse)
        .unwrap_or_default()
}

/// The index of the offered locale the first answerable of `requested` reads: the same tag, else
/// the same language.
fn best_match(offered: &[Locale], requested: &[LanguageIdentifier]) -> Option<usize> {
    requested.iter().find_map(|wanted| {
        offered
            .iter()
            .position(|locale| locale.0 == *wanted)
            .or_else(|| {
                offered
                    .iter()
                    .position(|locale| locale.0.language == wanted.language)
            })
    })
}

/// Finds the request panel's catalog and its user's locale; the first panel mounted on a router
/// installs it in the app context.
pub(crate) struct LocaleSource {
    pub(crate) catalog: for<'a> fn(&'a Cx) -> Option<&'a Catalog>,
    pub(crate) preferred: fn(&Cx) -> Option<String>,
}
