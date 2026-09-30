//! Naming helpers behind the default slugs and sidebar labels of a
//! [`Resource`](crate::resource::Resource) and a [`Page`](crate::Page).

use crate::schema::capitalize;

/// The last segment of a type's full path, e.g.
/// `tablo_core::resource::tests::UserResource` → `UserResource`.
pub(crate) fn type_short_name<T: ?Sized>() -> &'static str {
    let name = std::any::type_name::<T>();
    name.rsplit("::").next().unwrap_or(name)
}

/// A type's short name without `suffix` (`UserResource` → `User`), or the
/// whole name when stripping it would leave nothing.
pub(crate) fn type_stem<T: ?Sized>(suffix: &str) -> &'static str {
    let name = type_short_name::<T>();
    name.strip_suffix(suffix)
        .filter(|stem| !stem.is_empty())
        .unwrap_or(name)
}

/// A CamelCase identifier as a sentence-case phrase: `MediaLibrary` →
/// `Media library`.
pub(crate) fn sentence_case(name: &str) -> String {
    capitalize(&kebab_case(name).replace('-', " "))
}

/// Pluralize a capitalized English word with a compact ruleset (Filament
/// pluralizes via Laravel's `Str::plural`; this is the admin-grade subset):
/// a small irregular table (`person` → `people`, …), consonant-`y` → `ies`
/// (`Category` → `Categories`), sibilant endings → `es` (`Box` → `Boxes`),
/// `f`/`fe` → `ves` (`Knife` → `Knives`) with a few `+s` exceptions, and the
/// default `+s`.
///
/// A multi-word label pluralizes its last word only, so a noun phrase keeps its
/// head noun's rules: `Sales Person` → `Sales People`, `Blog Post` →
/// `Blog Posts`.
pub(crate) fn pluralize(label: &str) -> String {
    match label.rsplit_once(' ') {
        Some((head, last)) => format!("{head} {}", pluralize_word(last)),
        None => pluralize_word(label),
    }
}

/// [`pluralize`] for one word.
fn pluralize_word(word: &str) -> String {
    if word.is_empty() {
        return word.to_string();
    }
    let lower = word.to_lowercase();
    const IRREGULAR: &[(&str, &str)] = &[
        ("person", "people"),
        ("man", "men"),
        ("woman", "women"),
        ("child", "children"),
        ("mouse", "mice"),
        ("goose", "geese"),
        ("foot", "feet"),
        ("tooth", "teeth"),
        ("datum", "data"),
        ("criterion", "criteria"),
        ("index", "indices"),
        ("matrix", "matrices"),
        ("vertex", "vertices"),
        ("axis", "axes"),
        ("crisis", "crises"),
        ("analysis", "analyses"),
    ];
    if let Some((_, plural)) = IRREGULAR.iter().find(|(singular, _)| *singular == lower) {
        return match word.chars().next() {
            Some(first) if first.is_uppercase() => capitalize(plural),
            _ => (*plural).to_string(),
        };
    }
    // `f`/`fe` → `ves`, except the words that simply take `s`.
    const F_EXCEPTIONS: &[&str] = &["roof", "chief", "belief", "chef", "cliff", "cuff"];
    const UNCOUNTABLE: &[&str] = &[
        "fish",
        "sheep",
        "deer",
        "moose",
        "series",
        "species",
        "news",
        "equipment",
        "information",
        "rice",
    ];
    if UNCOUNTABLE.contains(&lower.as_str()) {
        return word.to_string();
    }
    if F_EXCEPTIONS.contains(&lower.as_str()) {
        format!("{word}s")
    } else if lower.ends_with('f') {
        format!("{}ves", &word[..word.len() - 1])
    } else if lower.ends_with("fe") {
        format!("{}ves", &word[..word.len() - 2])
    } else if lower.ends_with('y')
        && word.len() > 1
        && !"aeiou".contains(word.chars().nth(word.len() - 2).unwrap_or(' '))
    {
        format!("{}ies", &word[..word.len() - 1])
    } else if ["s", "ss", "sh", "ch", "x", "z"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
    {
        format!("{word}es")
    } else {
        format!("{word}s")
    }
}

/// Convert a CamelCase identifier to kebab-case: `BlogPost` → `blog-post`,
/// `APIKey` → `api-key`.
///
/// Delegates to `heck::ToKebabCase`: digits split words
/// (`User2FA` → `user2-fa`) and so do underscores (`Audit_Log` → `audit-log`).
/// Name resources and pages without underscores or override their `slug()`.
pub(crate) fn kebab_case(name: &str) -> String {
    use heck::ToKebabCase;
    name.to_kebab_case()
}

#[cfg(test)]
mod tests;
