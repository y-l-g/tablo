use super::*;
use crate::declaration::LocaleFault;

fn catalog(locales: &[&str], translations: &[(&str, &str)]) -> Result<Catalog, Vec<LocaleFault>> {
    Catalog::new(
        &locales
            .iter()
            .map(|tag| (*tag).to_owned())
            .collect::<Vec<_>>(),
        &translations
            .iter()
            .map(|(tag, source)| ((*tag).to_owned(), (*source).to_owned()))
            .collect::<Vec<_>>(),
    )
}

/// The offered locale at `tag`'s index spells `text`.
fn spell(catalog: &Catalog, tag: &str, text: &Text) -> String {
    let index = catalog
        .locales()
        .iter()
        .position(|locale| locale.to_string() == tag)
        .expect("the catalog offers the locale");
    catalog.format(index, text)
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
        faults(&["en", "not a tag"], &[]),
        [LocaleFault::Unparsed {
            tag: "not a tag".into()
        }]
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
    assert!(faults(&["fr"], &[("en", "x = y")]).is_empty());
    assert!(matches!(
        faults(&["en"], &[("en", "ok = fine\n= broken")]).as_slice(),
        [LocaleFault::Syntax { errors, .. }] if errors[0].starts_with("line 2:")
    ));
}

/// The locale of `offered` the `Accept-Language` value `header` reads.
fn negotiated(offered: &[&str], header: &str) -> Option<String> {
    let offered: Vec<Locale> = offered
        .iter()
        .map(|tag| Locale::parse(tag).expect("an offered tag parses"))
        .collect();
    locale::best_match(&offered, &locale::parse_accepted(header))
        .map(|index| offered[index].to_string())
}

#[test]
fn accept_language_reads_by_weight_and_skips_refused_languages() {
    let read = |header| negotiated(&["en", "fr"], header);
    assert_eq!(read("fr;q=0, en").as_deref(), Some("en"));
    assert_eq!(read("en;q=0.1, fr;q=0.9").as_deref(), Some("fr"));
    assert_eq!(read("de, fr;q=0.5, en;q=0.5").as_deref(), Some("fr"));
    assert_eq!(read("fr;q=2, en;q=0.x, en;q=0.5").as_deref(), Some("en"));
    assert_eq!(read("*, de").as_deref(), None);
    // A header past its first languages reads as those.
    let long = format!("{}fr", "de,".repeat(64));
    assert_eq!(read(&long), None);
}

#[test]
fn a_language_reads_the_offered_locale_of_its_script() {
    let read = |header| negotiated(&["zh-Hans", "zh-Hant", "fr"], header);
    assert_eq!(read("zh-HK").as_deref(), Some("zh-Hant"));
    assert_eq!(read("zh-TW").as_deref(), Some("zh-Hant"));
    assert_eq!(read("zh-CN").as_deref(), Some("zh-Hans"));
    assert_eq!(read("fr-CA").as_deref(), Some("fr"));
    assert_eq!(negotiated(&["en-GB", "fr"], "en").as_deref(), Some("en-GB"));
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
