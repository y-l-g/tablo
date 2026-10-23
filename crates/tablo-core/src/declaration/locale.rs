//! Why a panel's locales or translations cannot be served.

use std::fmt;

/// Why a panel's [`locales`](crate::Panel::locales) or
/// [`translations`](crate::Panel::translations) cannot be served.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LocaleFault {
    /// `Panel::locales` lists no locale.
    ///
    /// List one at least: the first is the panel's default.
    NoLocale,
    /// A locale is not a BCP 47 language tag.
    ///
    /// Spell it as `fr` or `pt-BR`.
    Unparsed {
        /// The refused tag.
        tag: String,
    },
    /// `Panel::locales` lists a locale twice.
    Duplicate {
        /// The repeated tag.
        tag: String,
    },
    /// `Panel::locales` lists a locale neither Tablo nor the app translates, so the panel would
    /// render it in English.
    ///
    /// Add its catalog with `Panel::translations`, or drop it.
    Untranslated {
        /// The locale.
        locale: String,
    },
    /// `Panel::translations` names a locale the panel does not offer, so no request reads it.
    ///
    /// Add it to `Panel::locales`, or drop the translations.
    Unoffered {
        /// The locale.
        locale: String,
    },
    /// A catalog `Panel::translations` adds is not valid Fluent.
    Syntax {
        /// The catalog's locale.
        locale: String,
        /// Each parse error, with the line it starts on.
        errors: Vec<String>,
    },
}

impl fmt::Display for LocaleFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoLocale => f.write_str(
                "`Panel::locales` lists no locale: list one at least, the first being the default",
            ),
            Self::Unparsed { tag } => write!(
                f,
                "locale '{tag}' is not a BCP 47 language tag: spell it as `fr` or `pt-BR`"
            ),
            Self::Duplicate { tag } => write!(f, "`Panel::locales` lists '{tag}' twice"),
            Self::Untranslated { locale } => write!(
                f,
                "locale '{locale}' has no catalog, so the panel would render it in English: add \
                 one with `Panel::translations(\"{locale}\", ..)`, or drop it from \
                 `Panel::locales`"
            ),
            Self::Unoffered { locale } => write!(
                f,
                "`Panel::translations` adds a catalog for '{locale}', which `Panel::locales` does \
                 not offer: add the locale, or drop the catalog"
            ),
            Self::Syntax { locale, errors } => write!(
                f,
                "the '{locale}' catalog is not valid Fluent: {}",
                errors.join("; ")
            ),
        }
    }
}
