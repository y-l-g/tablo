use topcoat::l10n::Locale;

use super::*;
use crate::declaration::LocaleFault;

fn parse(tag: &str) -> Locale {
    tag.parse().expect("a test's tag parses")
}

fn catalog(locales: &[&str], translations: &[(&str, &str)]) -> Result<Catalog, Vec<LocaleFault>> {
    Catalog::new(
        &locales.iter().map(|tag| parse(tag)).collect::<Vec<_>>(),
        &translations
            .iter()
            .map(|(tag, source)| (parse(tag), (*source).to_owned()))
            .collect::<Vec<_>>(),
    )
}

/// `tag` spells `text`.
fn spell(catalog: &Catalog, tag: &str, text: &Text) -> String {
    catalog.format(&parse(tag), text)
}

#[test]
fn a_locale_reads_its_own_catalog_then_the_default_then_english() {
    let catalog = catalog(
        &["fr", "de"],
        &[
            ("en", "only-english = English\nin-all = English"),
            ("fr", "only-french = Français\nin-all = Français"),
            ("de", "in-all = Deutsch"),
        ],
    )
    .expect("the catalog builds");
    assert_eq!(spell(&catalog, "de", &Text::message("in-all")), "Deutsch");
    assert_eq!(
        spell(&catalog, "de", &Text::message("only-french")),
        "Français"
    );
    assert_eq!(
        spell(&catalog, "de", &Text::message("only-english")),
        "English"
    );
}

/// English offered after a default reads English, never the default's catalogs, Tablo's or the
/// app's.
#[test]
fn english_after_the_default_reads_english() {
    let catalog = catalog(
        &["fr", "en"],
        &[(
            "fr",
            "tablo-shell-sign-out = Partir\nfield-page_count = Pages",
        )],
    )
    .expect("the catalog builds");
    let english = spell(
        Catalog::english(),
        "en",
        &Text::message("tablo-shell-sign-out"),
    );
    assert_eq!(
        spell(&catalog, "en", &Text::message("tablo-shell-sign-out")),
        english
    );
    assert_eq!(
        spell(&catalog, "fr", &Text::message("tablo-shell-sign-out")),
        "Partir"
    );
    assert_eq!(
        spell(&catalog, "en", &Text::field("page_count")),
        "Page count"
    );
}

#[test]
fn the_app_overrides_tablo_and_a_region_overrides_its_language() {
    let catalog = catalog(
        &["fr-CA"],
        &[
            ("fr", "greeting = Bonjour\nfarewell = Au revoir"),
            ("fr-CA", "greeting = Allô"),
            ("en", "tablo-shell-sign-out = Leave"),
        ],
    )
    .expect("the catalog builds");
    assert_eq!(spell(&catalog, "fr-CA", &Text::message("greeting")), "Allô");
    assert_eq!(
        spell(&catalog, "fr-CA", &Text::message("farewell")),
        "Au revoir"
    );
    // Tablo's French spells its own message over the app's English one.
    assert_ne!(
        spell(&catalog, "fr-CA", &Text::message("tablo-shell-sign-out")),
        "Leave"
    );
    let english = self::catalog(&["en"], &[("en", "tablo-shell-sign-out = Leave")])
        .expect("the catalog builds");
    assert_eq!(
        spell(&english, "en", &Text::message("tablo-shell-sign-out")),
        "Leave"
    );
}

#[test]
fn a_message_fills_its_arguments_and_selects_by_number() {
    let catalog = catalog(
        &["en"],
        &[(
            "en",
            "posts = { $count ->\n    [one] One post by { $author }\n   *[other] { $count } posts by { $author }\n}\nauthor = Ada\n    .short = A.",
        )],
    )
    .expect("the catalog builds");
    let posts = |count: usize| {
        Text::message("posts")
            .arg("count", count)
            .arg("author", Text::message("author.short"))
    };
    assert_eq!(spell(&catalog, "en", &posts(1)), "One post by A.");
    assert_eq!(spell(&catalog, "en", &posts(3)), "3 posts by A.");
}

#[test]
fn a_missing_message_renders_its_fallback_else_its_id() {
    let catalog = catalog(&["en"], &[]).expect("the catalog builds");
    assert_eq!(spell(&catalog, "en", &Text::message("nowhere")), "nowhere");
    let derived = Text::derived("field-word_count".to_owned(), "Word count".to_owned());
    assert_eq!(spell(&catalog, "en", &derived), "Word count");
    assert_eq!(
        spell(&catalog, "en", &Text::from("As written")),
        "As written"
    );
}

#[test]
fn the_pseudo_locale_accents_catalog_text_only() {
    let catalog =
        catalog(&["en-XA"], &[("en", "save = Save { $what }")]).expect("the catalog builds");
    let save = Text::message("save").arg("what", "Draft");
    assert_eq!(spell(&catalog, "en-XA", &save), "Šåṽé Draft");
}

#[test]
fn mount_refuses_what_no_request_could_read() {
    let faults = |locales: &[&str], translations: &[(&str, &str)]| {
        catalog(locales, translations).err().unwrap_or_default()
    };
    assert_eq!(faults(&[], &[]), [LocaleFault::NoLocale]);
    assert_eq!(
        faults(&["en", "en"], &[]),
        [LocaleFault::Duplicate { tag: "en".into() }]
    );
    assert_eq!(
        faults(&["he", "fr", "iw"], &[("he", "x = y")]),
        [LocaleFault::Duplicate { tag: "iw".into() }]
    );
    assert_eq!(
        faults(&["de"], &[]),
        [LocaleFault::Untranslated {
            locale: "de".into()
        }]
    );
    assert_eq!(
        faults(&["fr"], &[("de", "x = y")]),
        [LocaleFault::Unoffered {
            locale: "de".into()
        }]
    );
    // A catalog for a script's language serves none of that language's other scripts.
    assert_eq!(
        faults(&["zh-Hant"], &[("zh", "x = y")]),
        [
            LocaleFault::Unoffered {
                locale: "zh".into()
            },
            LocaleFault::Untranslated {
                locale: "zh-Hant".into()
            }
        ]
    );
    assert!(faults(&["fr"], &[("en", "x = y")]).is_empty());
    assert!(matches!(
        faults(&["en"], &[("en", "ok = fine\n= broken")]).as_slice(),
        [LocaleFault::Syntax { errors, .. }] if errors[0].starts_with("line 2:")
    ));
}

/// The locale entering a panel of `offered` installs for the `Accept-Language` value `header`.
fn negotiated(offered: &[&str], header: &str) -> String {
    let catalog = catalog(
        offered,
        &offered
            .iter()
            .map(|tag| (*tag, "greeting = Hi"))
            .collect::<Vec<_>>(),
    )
    .expect("the catalog builds");
    let (parts, ()) = http::Request::builder()
        .header(http::header::ACCEPT_LANGUAGE, header)
        .body(())
        .expect("the request builds")
        .into_parts();
    let cx = entered(&topcoat::context::Cx::default().with(parts), &catalog);
    topcoat::l10n::try_locale(&cx)
        .expect("entering installs a locale")
        .to_string()
}

#[test]
fn a_region_reads_the_offered_locale_of_its_script() {
    let read = |header| negotiated(&["fr", "zh-Hans", "zh-Hant"], header);
    assert_eq!(read("zh-HK"), "zh-Hant");
    assert_eq!(read("zh-TW"), "zh-Hant");
    assert_eq!(read("zh-CN"), "zh-Hans");
    assert_eq!(read("fr-CA"), "fr");
    assert_eq!(negotiated(&["fr", "en-GB"], "en"), "en-GB");
}

/// A header past its first languages reads as those, so a long one costs no more to match.
#[test]
fn a_long_accept_language_matches_its_first_languages() {
    let long = format!("{}en", "de,".repeat(64));
    assert_eq!(negotiated(&["fr", "en"], &long), "fr");
}

/// A locale no offered one is reads the offered locale it falls back to, else the default.
#[test]
fn a_locale_reads_the_offered_locale_it_falls_back_to() {
    let catalog = catalog(
        &["fr", "es", "es-419"],
        &[
            ("fr", "greeting = Bonjour"),
            ("es", "greeting = Hola\nfarewell = Adiós"),
            ("es-419", "greeting = Qué tal"),
        ],
    )
    .expect("the catalog builds");
    let greeting = Text::message("greeting");
    assert_eq!(spell(&catalog, "es-MX", &greeting), "Qué tal");
    assert_eq!(
        spell(&catalog, "es-MX", &Text::message("farewell")),
        "Adiós"
    );
    assert_eq!(spell(&catalog, "es-ES", &greeting), "Hola");
    assert_eq!(spell(&catalog, "ja", &greeting), "Bonjour");
}

/// A message an upper layer redefines without an attribute reads the attribute from below.
#[test]
fn an_attribute_a_locale_leaves_out_reads_from_below() {
    let catalog = catalog(
        &["fr"],
        &[
            ("en", "resource-person = Person\n    .plural = People"),
            ("fr", "resource-person = Personne"),
        ],
    )
    .expect("the catalog builds");
    let person = Text::model::<Person>();
    assert_eq!(spell(&catalog, "fr", &person), "Personne");
    assert_eq!(spell(&catalog, "fr", &person.plural()), "People");
}

struct Person;

/// A resource's plural follows its label: a plain label pluralized, a message's attribute
/// `plural`, else its id.
#[test]
fn a_labels_plural_follows_the_label() {
    let catalog = catalog(
        &["en"],
        &[(
            "en",
            "book-label = Volume\n    .plural = Volumes\nbare = Bare",
        )],
    )
    .expect("the catalog builds");
    let plural = |text: Text| spell(&catalog, "en", &text.plural());
    assert_eq!(plural(Text::from("Category")), "Categories");
    assert_eq!(plural(Text::message("book-label")), "Volumes");
    assert_eq!(plural(Text::message("bare")), "bare.plural");
    assert_eq!(plural(Text::model::<Person>()), "People");
}

#[test]
fn a_catalog_calls_fluents_functions() {
    let catalog = catalog(
        &["en"],
        &[(
            "en",
            "price = { NUMBER($amount, minimumFractionDigits: 2) }",
        )],
    )
    .expect("the catalog builds");
    let price = Text::message("price").arg("amount", 3);
    assert_eq!(spell(&catalog, "en", &price), "3.00");
}
