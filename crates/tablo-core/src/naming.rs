//! Naming helpers behind the default slugs and sidebar labels of a
//! [`Resource`](crate::resource::Resource), a [`Page`](crate::Page) and a field label, and the
//! prefix and route paths a panel mounts under.

/// The last segment of a type's full path, without its generic arguments, e.g.
/// `tablo_core::resource::tests::UserResource` → `UserResource`, `app::Count<app::Book>` →
/// `Count`.
pub(crate) fn type_short_name<T: ?Sized>() -> &'static str {
    let name = std::any::type_name::<T>();
    let name = name.split_once('<').map_or(name, |(path, _)| path);
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

/// The id of the message an app defines to translate the label Tablo derives from `name`: `kind`,
/// a dash, then `name` with each character a Fluent id cannot hold read as a dash.
/// `("action", "send/invite")` is `action-send-invite`.
pub(crate) fn derived_id(kind: &str, name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("{kind}-{name}")
}

/// A CamelCase identifier as a sentence-case phrase: `MediaLibrary` →
/// `Media library`. A word the identifier spells in capitals keeps them:
/// `APIKey` → `API key`.
pub(crate) fn sentence_case(name: &str) -> String {
    // `kebab_case` drops every non-alphanumeric character, so its words, in order, lowercase
    // the identifier's alphanumeric runs; a word whose run fails that check reads lowercase.
    let mut source = name.chars().filter(|c| c.is_alphanumeric());
    let words: Vec<String> = kebab_case(name)
        .split('-')
        .map(|word| {
            let original: String = source.by_ref().take(word.chars().count()).collect();
            let acronym = original.chars().filter(|c| c.is_alphabetic()).count() > 1
                && !original.chars().any(char::is_lowercase);
            if acronym && original.to_lowercase() == word {
                original
            } else {
                word.to_string()
            }
        })
        .collect();
    capitalize(&words.join(" "))
}

/// Pluralize an English noun phrase through the `pluralizer` crate, the rules Toasty names its
/// tables with: irregulars (`Person` → `People`), uncountables (`Staff`), `f`/`fe` → `ves` where
/// English takes it (`Knife` → `Knives`, `Cafe` → `Cafes`) and the rest.
///
/// A multi-word label pluralizes its last word only, so a noun phrase keeps its head noun's
/// rules: `Sales person` → `Sales people`, `Blog post` → `Blog posts`. The word keeps its case: a
/// capitalized word stays capitalized, and an acronym keeps its capitals (`URL` → `URLs`).
pub(crate) fn pluralize(label: &str) -> String {
    match label.rsplit_once(' ') {
        Some((head, last)) => format!("{head} {}", pluralize_word(last)),
        None => pluralize_word(label),
    }
}

/// [`pluralize`] for one word.
fn pluralize_word(word: &str) -> String {
    let lower = word.to_lowercase();
    let plural = pluralizer::pluralize(&lower, 2, false);
    let acronym = word.chars().filter(|c| c.is_alphabetic()).count() > 1
        && !word.chars().any(char::is_lowercase);
    match plural.strip_prefix(lower.as_str()) {
        // An acronym keeps its letters and takes a lowercase suffix: `URL` → `URLs`.
        Some(suffix) if acronym => format!("{word}{suffix}"),
        _ if acronym => plural.to_uppercase(),
        _ if word.starts_with(char::is_uppercase) => capitalize(&plural),
        _ => plural,
    }
}

/// A phrase [`sentence_case`] and [`pluralize`] spelled, as a slug: its words lowercased and
/// joined by hyphens. `Contact people` → `contact-people`; an acronym's plural stays one word,
/// `URLs` → `urls`, where [`kebab_case`] would split it at its last capital.
pub(crate) fn slug_case(phrase: &str) -> String {
    phrase.to_lowercase().replace(' ', "-")
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

/// A field's human label from its storage name.
///
/// Sentence case, with the underscores a Rust column name carries read as
/// spaces: `word_count` is "Word count", not "Word_count". A label is the one
/// place a column name becomes prose, so it should not leak the identifier. An
/// explicit [`.label(..)`](crate::schema::TextField::label) still wins.
pub(crate) fn capitalize(s: &str) -> String {
    let spaced = s.replace('_', " ");
    let mut c = spaced.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

/// The prefix [`Panel::new`](crate::Panel::new) mounts `raw` at: trimmed, one leading slash, no
/// trailing slash, and `/admin` when nothing is left.
pub(crate) fn normalize_prefix(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.is_empty() {
        "/admin".to_string()
    } else {
        format!("/{trimmed}")
    }
}

/// The prefix `path`'s first segment names, else `/admin`: a request's panel prefix outside every
/// panel.
pub(crate) fn path_prefix(path: &str) -> String {
    path.split('/')
        .nth(1)
        .filter(|segment| !segment.is_empty())
        .map_or_else(|| "/admin".to_string(), |segment| format!("/{segment}"))
}

/// Parses a panel route path, panicking on malformed input.
pub(crate) fn route_path(path: &str) -> topcoat::router::PathBuf {
    topcoat::router::Path::from_str(path)
        .expect("panel route paths are well-formed")
        .to_owned()
}

/// Path segment of the list page's create page, under [`DASH_ROUTE_SEGMENT`].
pub(crate) const CREATE_ROUTE_SEGMENT: &str = "create";

/// Static segment guarding the list's own routes, and a record's action and relation routes, from
/// record keys: `matchit` prefers a static segment over `{id}`, so a record whose key is `create`
/// would otherwise lose its detail page to the create form.
pub(crate) const DASH_ROUTE_SEGMENT: &str = "-";

/// A route of the list itself, `{list}/-/{segment}`: the create form, the export, the option
/// search, the bulk delete.
pub(crate) fn list_route(list_path: &str, segment: &str) -> String {
    format!(
        "{}/{DASH_ROUTE_SEGMENT}/{segment}",
        list_path.trim_end_matches('/')
    )
}

/// The list page's create link: `{list_path}/-/create`.
pub(crate) fn create_page_url(list_path: &str) -> String {
    list_route(list_path, CREATE_ROUTE_SEGMENT)
}

#[cfg(test)]
mod tests;
