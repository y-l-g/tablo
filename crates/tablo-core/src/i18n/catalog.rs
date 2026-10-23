//! A panel's [`Catalog`]: one Fluent bundle per locale it offers, Tablo's messages under the app's.

use std::{
    borrow::Cow,
    sync::{Arc, LazyLock},
};

use fluent_bundle::{FluentArgs, FluentResource, FluentValue, concurrent::FluentBundle};

use super::{
    Locale,
    text::{Message, Repr, Text},
};
use crate::declaration::LocaleFault;

type Bundle = FluentBundle<Arc<FluentResource>>;

/// The catalogs Tablo ships, by locale. `en` holds every message Tablo renders; a test holds
/// each other catalog to the same messages and variables.
const SHIPPED: &[(&str, &str)] = &[
    ("en", include_str!("../../locales/en.ftl")),
    ("fr", include_str!("../../locales/fr.ftl")),
];

/// The pseudo-locale: English with every catalog letter swapped for an accented one, so a word
/// that reads plain on its page is one no catalog spells.
const PSEUDO: &str = "en-XA";

/// [`SHIPPED`], parsed.
static PARSED: LazyLock<Vec<(Locale, Arc<FluentResource>)>> = LazyLock::new(|| {
    SHIPPED
        .iter()
        .map(|(tag, source)| {
            let locale = Locale::parse(tag).expect("a shipped catalog's tag parses");
            let resource = FluentResource::try_new((*source).to_owned())
                .unwrap_or_else(|(_, errors)| panic!("the {tag} catalog parses: {errors:?}"));
            (locale, Arc::new(resource))
        })
        .collect()
});

/// The catalog of what renders outside every panel: Tablo's English.
static ENGLISH: LazyLock<Catalog> =
    LazyLock::new(|| Catalog::new(&["en".to_owned()], &[]).expect("Tablo's catalog builds"));

/// The messages a panel renders in each locale it offers.
pub(crate) struct Catalog {
    /// The offered locales, the default first.
    locales: Vec<Locale>,
    /// Each offered locale's bundle, in `locales` order.
    bundles: Vec<Bundle>,
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
        locales: &[String],
        translations: &[(String, String)],
    ) -> Result<Self, Vec<LocaleFault>> {
        let mut faults = Vec::new();
        let mut offered: Vec<Locale> = Vec::new();
        for tag in locales {
            match Locale::parse(tag) {
                None => faults.push(LocaleFault::Unparsed { tag: tag.clone() }),
                Some(locale) if offered.contains(&locale) => {
                    faults.push(LocaleFault::Duplicate { tag: tag.clone() });
                }
                Some(locale) => offered.push(locale),
            }
        }
        if locales.is_empty() {
            faults.push(LocaleFault::NoLocale);
        }
        let english = Locale::parse("en").expect("`en` parses");
        let mut added: Vec<(Locale, Arc<FluentResource>)> = Vec::new();
        for (tag, source) in translations {
            let Some(locale) = Locale::parse(tag) else {
                faults.push(LocaleFault::Unparsed { tag: tag.clone() });
                continue;
            };
            if !locale.serves(&english) && !offered.iter().any(|o| locale.serves(o)) {
                faults.push(LocaleFault::Unoffered {
                    locale: tag.clone(),
                });
            }
            match FluentResource::try_new(source.clone()) {
                Ok(resource) => added.push((locale, Arc::new(resource))),
                Err((_, errors)) => faults.push(LocaleFault::Syntax {
                    locale: tag.clone(),
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
            let pseudo = locale.to_string() == PSEUDO;
            let translated = pseudo
                || PARSED
                    .iter()
                    .chain(&added)
                    .any(|(catalog, _)| catalog.serves(locale));
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
                let below = if locale.language() == english.language() {
                    vec![english.clone()]
                } else {
                    vec![english.clone(), default.clone()]
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
        &ENGLISH
    }

    /// The offered locales, the default first.
    pub(crate) fn locales(&self) -> &[Locale] {
        &self.locales
    }

    /// `text` as the offered locale at `index` spells it.
    pub(crate) fn format(&self, index: usize, text: &Text) -> String {
        format(&self.bundles[index], text)
    }
}

/// The bundle of `locale`: each layer of `below`, then `locale`, adds Tablo's catalogs serving it,
/// then the app's, so the later one overrides a message both hold. A catalog serving several
/// layers sits at the last: `en` stays over a default `fr` for a request reading `en-GB`.
fn bundle(locale: &Locale, below: &[Locale], added: &[(Locale, Arc<FluentResource>)]) -> Bundle {
    let mut bundle = Bundle::new_concurrent(vec![locale.0.clone()]);
    // Placeables render into HTML attributes and plain text, where the isolation marks Fluent
    // wraps them in by default show as stray characters.
    bundle.set_use_isolating(false);
    if locale.to_string() == PSEUDO {
        bundle.set_transform(Some(pseudo));
    }
    // From the top layer down, so a catalog lands in the last layer it serves.
    let mut placed: Vec<&Arc<FluentResource>> = Vec::new();
    let mut layers: Vec<Vec<&Arc<FluentResource>>> = Vec::new();
    for layer in below.iter().chain([locale]).rev() {
        let mut resources = Vec::new();
        // A language-wide catalog goes under the region's, so `fr-CA` overrides `fr`.
        for catalogs in [PARSED.as_slice(), added] {
            let mut serving: Vec<&(Locale, Arc<FluentResource>)> = catalogs
                .iter()
                .filter(|(catalog, _)| catalog.serves(layer))
                .collect();
            serving.sort_by_key(|(catalog, _)| *catalog == *layer);
            for (_, resource) in serving {
                if !placed.iter().any(|done| Arc::ptr_eq(done, resource)) {
                    placed.push(resource);
                    resources.push(resource);
                }
            }
        }
        layers.push(resources);
    }
    for resource in layers.into_iter().rev().flatten() {
        bundle.add_resource_overriding(Arc::clone(resource));
    }
    bundle
}

/// `text` as `bundle` spells it.
fn format(bundle: &Bundle, text: &Text) -> String {
    match &text.0 {
        Repr::Plain(plain) => plain.to_string(),
        Repr::Integer(n) => n.to_string(),
        Repr::Message(message) => format_message(bundle, message).unwrap_or_else(|| {
            tracing::debug!(id = %message.id, "no catalog holds the message");
            message
                .fallback
                .as_deref()
                .unwrap_or(&message.id)
                .to_owned()
        }),
    }
}

/// `message` as `bundle` spells it; `None` when the bundle holds neither it nor its attribute.
fn format_message(bundle: &Bundle, message: &Message) -> Option<String> {
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
            _ => FluentValue::from(format(bundle, value)),
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
