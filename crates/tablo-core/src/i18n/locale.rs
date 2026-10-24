//! The request's [`Locale`]: which of its panel's locales it reads.

use std::{fmt, sync::OnceLock};

use fluent_langneg::{NegotiationStrategy, negotiate::filter_matches};
use topcoat::context::{Cx, try_app_context, try_request_context};
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

/// The locale the request reads its words in: a mail's recipient's, else the signed-in user's
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
    let preferred = try_request_context::<Addressee>(cx)
        .and_then(|addressee| addressee.0.as_deref())
        .or_else(|| (source.preferred)(cx))
        .and_then(Locale::parse)
        .and_then(|tag| best_match(locales, &[tag.0]));
    let index = preferred
        .or_else(|| match try_request_context::<AcceptedLocale>(cx) {
            // The header is the request's own, so its match is read once.
            Some(memo) => *memo.0.get_or_init(|| best_match(locales, &accepted(cx))),
            None => best_match(locales, &accepted(cx)),
        })
        .unwrap_or(0);
    (catalog, index)
}

/// `cx` writing to an account whose [`locale`](crate::PanelUser::locale) is `locale`, such as a
/// mail's recipient: its words read that locale when the panel offers it, else the request's.
pub(crate) fn addressed_to(cx: &Cx, locale: Option<&str>) -> Cx {
    cx.with(Addressee(locale.map(str::to_owned)))
}

/// The locale of the account the request writes to, which [`addressed_to`] installs.
struct Addressee(Option<String>);

/// The offered locale the request's `Accept-Language` reads, once read: entering a panel installs
/// it empty in the request context.
#[derive(Debug, Clone, Default)]
pub(crate) struct AcceptedLocale(OnceLock<Option<usize>>);

/// The most languages of an `Accept-Language` read, so a long header costs no more than a
/// browser's.
const MAX_ACCEPTED: usize = 16;

/// The languages the request's `Accept-Language` asks for, the most wanted first: by weight, then
/// in the header's order, without those weighing `q=0`, which the browser refuses.
fn accepted(cx: &Cx) -> Vec<LanguageIdentifier> {
    try_request_context::<http::request::Parts>(cx)
        .and_then(|parts| parts.headers.get(http::header::ACCEPT_LANGUAGE))
        .and_then(|value| value.to_str().ok())
        .map(parse_accepted)
        .unwrap_or_default()
}

/// The languages of the `Accept-Language` value `header`, as [`accepted`] orders them.
pub(super) fn parse_accepted(header: &str) -> Vec<LanguageIdentifier> {
    let mut weighted: Vec<(u16, LanguageIdentifier)> = header
        .split(',')
        .take(MAX_ACCEPTED)
        .filter_map(|range| {
            let mut params = range.split(';');
            let tag = params.next()?.trim().parse().ok()?;
            let weight = match params.find_map(|param| param.trim().strip_prefix("q=")) {
                Some(q) => weight(q)?,
                None => 1000,
            };
            (weight > 0).then_some((weight, tag))
        })
        .collect();
    // Stable, so languages of one weight keep the header's order.
    weighted.sort_by_key(|(weight, _)| std::cmp::Reverse(*weight));
    weighted.into_iter().map(|(_, tag)| tag).collect()
}

/// The weight `q` in thousandths: `0` to `1` with at most three decimals; `None` when malformed.
fn weight(q: &str) -> Option<u16> {
    let (whole, fraction) = q.split_once('.').unwrap_or((q, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let thousandths: u16 = format!("{fraction:0<3}").parse().ok()?;
    match whole {
        "0" => Some(thousandths),
        "1" if thousandths == 0 => Some(1000),
        _ => None,
    }
}

/// The index of the offered locale the first answerable of `requested` reads: the same tag, else
/// the closest of the same language, its script told by likely subtags, so `zh-HK` reads
/// `zh-Hant` and `fr-CA` reads `fr`.
pub(super) fn best_match(offered: &[Locale], requested: &[LanguageIdentifier]) -> Option<usize> {
    let requested: Vec<_> = requested.iter().filter_map(negotiable).collect();
    let tags: Vec<(usize, NegotiableTag)> = offered
        .iter()
        .enumerate()
        .filter_map(|(index, locale)| Some((index, negotiable(&locale.0)?)))
        .collect();
    let available: Vec<&NegotiableTag> = tags.iter().map(|(_, tag)| tag).collect();
    let found = *filter_matches(&requested, &available, NegotiationStrategy::Lookup).first()?;
    tags.iter()
        .find(|(_, tag)| std::ptr::eq(tag, *found))
        .map(|(index, _)| *index)
}

/// The tag type `fluent-langneg` negotiates in, which its 0.14 release moved off `unic-langid`.
type NegotiableTag = fluent_langneg::LanguageIdentifier;

/// `tag` in the type `fluent-langneg` negotiates in; `None` when that type will not read it.
fn negotiable(tag: &LanguageIdentifier) -> Option<NegotiableTag> {
    tag.to_string().parse().ok()
}

/// Finds the request panel's catalog and its user's locale; the first panel mounted on a router
/// installs it in the app context.
pub(crate) struct LocaleSource {
    pub(crate) catalog: for<'a> fn(&'a Cx) -> Option<&'a Catalog>,
    pub(crate) preferred: for<'a> fn(&'a Cx) -> Option<&'a str>,
}
