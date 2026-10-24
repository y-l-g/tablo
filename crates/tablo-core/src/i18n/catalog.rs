//! A panel's [`Catalog`]: one Fluent bundle per locale it offers, Tablo's messages under the app's.

use std::{
    borrow::Cow,
    sync::{Arc, LazyLock},
};

use fluent_bundle::{FluentArgs, FluentResource, FluentValue, concurrent::FluentBundle};
use topcoat::l10n::{Locale, locale, lookup};

use super::{
    ENGLISH,
    text::{Message, Repr, Text},
};
use crate::declaration::LocaleFault;

type Bundle = FluentBundle<Arc<FluentResource>>;

/// A locale's bundles, the whole stack first, then without its top layer, and so on: a message
/// an upper layer redefines without one of its attributes reads that attribute from below.
struct Layered(Vec<Bundle>);

/// The catalogs Tablo ships, by locale. `en` holds every message Tablo renders; a test holds
/// each other catalog to the same messages and variables.
const SHIPPED: &[(Locale, &str)] = &[
    (locale!("en"), include_str!("../../locales/en.ftl")),
    (locale!("fr"), include_str!("../../locales/fr.ftl")),
];

/// The pseudo-locale: English with every catalog letter swapped for an accented one, so a word
/// that reads plain on its page is one no catalog spells.
const PSEUDO: Locale = locale!("en-XA");

/// [`SHIPPED`], parsed.
static PARSED: LazyLock<Vec<(Locale, Arc<FluentResource>)>> = LazyLock::new(|| {
    SHIPPED
        .iter()
        .map(|(locale, source)| {
            let resource = FluentResource::try_new((*source).to_owned())
                .unwrap_or_else(|(_, errors)| panic!("the {locale} catalog parses: {errors:?}"));
            (locale.clone(), Arc::new(resource))
        })
        .collect()
});

/// The catalog of what renders outside every panel: Tablo's English.
static TABLO_ENGLISH: LazyLock<Catalog> = LazyLock::new(|| {
    Catalog::new(std::slice::from_ref(&ENGLISH), &[]).expect("Tablo's catalog builds")
});

/// The messages a panel renders in each locale it offers.
pub(crate) struct Catalog {
    /// The offered locales, the default first.
    locales: Vec<Locale>,
    /// Each offered locale's bundles, in `locales` order.
    bundles: Vec<Layered>,
}

impl Catalog {
    /// The catalog of a panel offering `locales`, the default first, with the app's
    /// `translations` over Tablo's.
    ///
    /// A locale's bundle layers English, then the default locale, then the locale itself, each as
    /// Tablo's catalog under the app's: a message one layer leaves out reads from the layer below.
    /// An English locale skips the default: English, which a derived label's fallback spells too,
    /// is its own.
    pub(crate) fn new(
        locales: &[Locale],
        translations: &[(Locale, String)],
    ) -> Result<Self, Vec<LocaleFault>> {
        let mut faults = Vec::new();
        let mut offered: Vec<Locale> = Vec::new();
        for locale in locales {
            // `iw` and `he` name one language, so a panel offering both offers it twice.
            let canonical = locale.clone().canonicalize();
            if offered
                .iter()
                .any(|o| o.clone().canonicalize() == canonical)
            {
                faults.push(LocaleFault::Duplicate {
                    tag: locale.to_string(),
                });
            } else {
                offered.push(locale.clone());
            }
        }
        if locales.is_empty() {
            faults.push(LocaleFault::NoLocale);
        }
        let mut added: Vec<(Locale, Arc<FluentResource>)> = Vec::new();
        for (locale, source) in translations {
            if !serves(locale, &ENGLISH) && !offered.iter().any(|o| serves(locale, o)) {
                faults.push(LocaleFault::Unoffered {
                    locale: locale.to_string(),
                });
            }
            match FluentResource::try_new(source.clone()) {
                Ok(resource) => added.push((locale.clone(), Arc::new(resource))),
                Err((_, errors)) => faults.push(LocaleFault::Syntax {
                    locale: locale.to_string(),
                    errors: errors
                        .iter()
                        .map(|error| {
                            let line = source[..error.pos.start.min(source.len())]
                                .matches('\n')
                                .count()
                                + 1;
                            format!("line {line}: {}", error.kind)
                        })
                        .collect(),
                }),
            }
        }
        for locale in &offered {
            let translated = *locale == PSEUDO
                || PARSED
                    .iter()
                    .chain(&added)
                    .any(|(catalog, _)| serves(catalog, locale));
            if !translated {
                faults.push(LocaleFault::Untranslated {
                    locale: locale.to_string(),
                });
            }
        }
        if !faults.is_empty() {
            return Err(faults);
        }
        let default = offered[0].clone();
        let bundles = offered
            .iter()
            .map(|locale| {
                let below = if locale.id().language() == ENGLISH.id().language() {
                    vec![ENGLISH.clone()]
                } else {
                    vec![ENGLISH.clone(), default.clone()]
                };
                bundle(locale, &below, &added)
            })
            .collect();
        Ok(Self {
            locales: offered,
            bundles,
        })
    }

    /// The catalog of what renders outside every panel: Tablo's English.
    pub(crate) fn english() -> &'static Self {
        &TABLO_ENGLISH
    }

    /// The offered locales, the default first.
    pub(crate) fn locales(&self) -> &[Locale] {
        &self.locales
    }

    /// `text` as `locale` spells it: the offered locale itself, else the offered locale that
    /// serves it, as `fr` serves `fr-CA`, else the default.
    pub(crate) fn format(&self, locale: &Locale, text: &Text) -> String {
        let index = self
            .locales
            .iter()
            .position(|offered| offered == locale)
            .or_else(|| {
                let served = lookup(locale, &self.locales)?;
                self.locales.iter().position(|offered| offered == served)
            })
            .unwrap_or(0);
        format(&self.bundles[index], text)
    }
}

/// Whether a catalog for `catalog` serves `target`: `target` itself, or a locale `target` falls
/// back to, as `fr` serves `fr-CA` and `es-419` serves `es-MX`.
fn serves(catalog: &Locale, target: &Locale) -> bool {
    lookup(target, std::slice::from_ref(catalog)).is_some()
}

/// The bundles of `locale`: each layer of `below`, then `locale`, adds Tablo's catalogs serving
/// it, then the app's, so the later one overrides a message both hold. A catalog serving several
/// layers sits at the last: `en` stays over a default `fr` for a request reading `en-GB`.
fn bundle(locale: &Locale, below: &[Locale], added: &[(Locale, Arc<FluentResource>)]) -> Layered {
    // From the top layer down, so a catalog lands in the last layer it serves.
    let mut placed: Vec<&Arc<FluentResource>> = Vec::new();
    let mut layers: Vec<Vec<&Arc<FluentResource>>> = Vec::new();
    for layer in below.iter().chain([locale]).rev() {
        let mut resources = Vec::new();
        // A language-wide catalog goes under the region's, so `fr-CA` overrides `fr`.
        for catalogs in [PARSED.as_slice(), added] {
            for (_, resource) in least_specific_first(layer, catalogs) {
                if !placed.iter().any(|done| Arc::ptr_eq(done, resource)) {
                    placed.push(resource);
                    resources.push(resource);
                }
            }
        }
        if !resources.is_empty() {
            layers.push(resources);
        }
    }
    layers.reverse();
    Layered(
        (1..=layers.len())
            .rev()
            .map(|height| {
                let tag = locale
                    .id()
                    .to_string()
                    .parse()
                    .expect("`unic-langid` reads every BCP 47 language identifier");
                let mut bundle = Bundle::new_concurrent(vec![tag]);
                // Placeables render into HTML attributes and plain text, where the isolation
                // marks Fluent wraps them in by default show as stray characters.
                bundle.set_use_isolating(false);
                if *locale == PSEUDO {
                    bundle.set_transform(Some(pseudo));
                }
                bundle
                    .add_builtins()
                    .expect("a new bundle holds no function");
                for resource in layers[..height].iter().flatten() {
                    bundle.add_resource_overriding(Arc::clone(resource));
                }
                bundle
            })
            .collect(),
    )
}

/// The catalogs of `catalogs` serving `layer`, the one furthest along its fallback chain first,
/// so `fr` comes before `fr-CA`; catalogs of one locale keep their order.
fn least_specific_first<'a>(
    layer: &Locale,
    catalogs: &'a [(Locale, Arc<FluentResource>)],
) -> Vec<&'a (Locale, Arc<FluentResource>)> {
    let mut remaining: Vec<Locale> = Vec::new();
    for (catalog, _) in catalogs {
        if serves(catalog, layer) && !remaining.contains(catalog) {
            remaining.push(catalog.clone());
        }
    }
    let mut chain: Vec<Locale> = Vec::new();
    while let Some(next) = lookup(layer, &remaining).cloned() {
        remaining.retain(|locale| *locale != next);
        chain.push(next);
    }
    chain
        .iter()
        .rev()
        .flat_map(|locale| {
            catalogs
                .iter()
                .filter(move |(catalog, _)| catalog == locale)
        })
        .collect()
}

/// `text` as `layered` spells it.
fn format(layered: &Layered, text: &Text) -> String {
    match &text.0 {
        Repr::Plain(plain) => plain.to_string(),
        Repr::Integer(n) => n.to_string(),
        Repr::Message(message) => layered
            .0
            .iter()
            .find_map(|bundle| format_message(layered, bundle, message))
            .unwrap_or_else(|| {
                tracing::debug!(id = %message.id, "no catalog holds the message");
                message
                    .fallback
                    .as_deref()
                    .unwrap_or(&message.id)
                    .to_owned()
            }),
    }
}

/// `message` as `bundle` spells it, its `Text` arguments as `layered` does; `None` when the
/// bundle holds neither it nor its attribute.
fn format_message(layered: &Layered, bundle: &Bundle, message: &Message) -> Option<String> {
    let (id, attribute) = match message.id.split_once('.') {
        Some((id, attribute)) => (id, Some(attribute)),
        None => (&*message.id, None),
    };
    let found = bundle.get_message(id)?;
    let pattern = match attribute {
        Some(attribute) => found.get_attribute(attribute)?.value(),
        None => found.value()?,
    };
    let mut args = FluentArgs::with_capacity(message.args.len());
    for (name, value) in &message.args {
        let value = match &value.0 {
            Repr::Integer(n) => FluentValue::from(*n),
            _ => FluentValue::from(format(layered, value)),
        };
        args.set(Cow::clone(name), value);
    }
    let mut errors = Vec::new();
    let spelled = bundle.format_pattern(pattern, Some(&args), &mut errors);
    if !errors.is_empty() {
        tracing::debug!(id = %message.id, ?errors, "a message formatted with errors");
    }
    Some(spelled.into_owned())
}

/// The pseudo-locale's spelling of a catalog's text: each ASCII letter swapped for an accented
/// one, the rest kept.
fn pseudo(text: &str) -> Cow<'_, str> {
    const LOWER: [char; 26] = [
        'å', 'ƀ', 'ç', 'ð', 'é', 'ƒ', 'ĝ', 'ĥ', 'î', 'ĵ', 'ķ', 'ļ', 'ɱ', 'ñ', 'ö', 'þ', 'ǫ', 'ŕ',
        'š', 'ţ', 'û', 'ṽ', 'ŵ', 'ẋ', 'ý', 'ž',
    ];
    const UPPER: [char; 26] = [
        'Å', 'Ɓ', 'Ç', 'Ð', 'É', 'Ƒ', 'Ĝ', 'Ĥ', 'Î', 'Ĵ', 'Ķ', 'Ļ', 'Ṁ', 'Ñ', 'Ö', 'Þ', 'Ǫ', 'Ŕ',
        'Š', 'Ţ', 'Û', 'Ṽ', 'Ŵ', 'Ẋ', 'Ý', 'Ž',
    ];
    Cow::Owned(
        text.chars()
            .map(|c| match c {
                'a'..='z' => LOWER[usize::from(c as u8 - b'a')],
                'A'..='Z' => UPPER[usize::from(c as u8 - b'A')],
                _ => c,
            })
            .collect(),
    )
}
