//! [`PageDef::filter`](crate::PageDef::filter): the values a page offers above its widgets, held
//! in its query string, which its widgets read through [`page_filters`](crate::page_filters).

use jiff::civil::Date;

use crate::{IntoOptions, Text};

/// What starts every page filter's query parameter.
pub(crate) const PREFIX: &str = "pf.";

/// A value a page offers above its widgets and keeps in its query string, which its widgets read
/// through [`page_filters`](crate::page_filters): the period a dashboard's figures cover, or the
/// store they count.
///
/// A select holds `?pf.<key>=<value>`, one of its options; a date range holds `?pf.<key>.from=`
/// and `?pf.<key>.to=`, each a `YYYY-MM-DD` day. The `pf.` prefix keeps them apart from a table's
/// `f.` filters on the same page. The bar is a form the page submits to itself, carrying the
/// request's other parameters, so a filtered dashboard is a link, a table on it keeps its search
/// and sort, and it works without JavaScript.
///
/// A page filter narrows nothing by itself: each widget decides what a value means, and still
/// reads its records through [`scoped_query`](crate::scoped_query) or
/// [`list_query`](crate::list_query), so a value cannot widen what the request may see.
///
/// ```rust
/// use tablo_core::{Page, PageDef, PageFilter};
///
/// struct SalesPage;
///
/// impl Page for SalesPage {
///     fn declare() -> PageDef {
///         PageDef::new()
///             .filter(PageFilter::date_range("period", "Period"))
///             .filter(PageFilter::select(
///                 "channel",
///                 "Channel",
///                 [("web", "Web"), ("store", "Store")],
///             ))
///     }
/// }
/// ```
#[derive(Clone)]
pub struct PageFilter {
    key: String,
    pub(crate) label: Text,
    pub(crate) kind: Kind,
}

/// What a page filter holds.
#[derive(Clone)]
pub(crate) enum Kind {
    /// One of these `(value, label)` options.
    Select(Vec<(String, Text)>),
    /// A first and a last day, either one open.
    DateRange,
}

impl PageFilter {
    /// A select over `options`, held in `?pf.<key>=`: values that are their own label, `(value,
    /// label)` pairs, or a [`#[derive(Options)]`](crate::Options) enum's options. A value naming
    /// no option reads as none.
    pub fn select(
        key: impl Into<String>,
        label: impl Into<Text>,
        options: impl IntoOptions,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind: Kind::Select(options.into_options()),
        }
    }

    /// A range of days, held in `?pf.<key>.from=` and `?pf.<key>.to=`. A day that does not parse
    /// reads as an open end.
    pub fn date_range(key: impl Into<String>, label: impl Into<Text>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind: Kind::DateRange,
        }
    }

    /// The key its query parameters carry.
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    /// The query parameter holding the filter's value, or the `end` of its range.
    pub(crate) fn param(&self, end: Option<&str>) -> String {
        match end {
            Some(end) => format!("{PREFIX}{}.{end}", self.key),
            None => format!("{PREFIX}{}", self.key),
        }
    }

    /// Whether the key is a word a query parameter carries as it is: ASCII letters, digits, `-`
    /// and `_`, with no `.`, which separates a range's ends.
    pub(crate) fn has_valid_key(&self) -> bool {
        !self.key.is_empty()
            && self
                .key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    }

    /// What `pairs`, a query string's pairs, hold for this filter.
    fn read(&self, pairs: &[(String, String)]) -> Option<Held> {
        let param = |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.trim())
                .filter(|value| !value.is_empty())
        };
        match &self.kind {
            Kind::Select(options) => {
                let value = param(&self.param(None))?;
                options
                    .iter()
                    .any(|(option, _)| option == value)
                    .then(|| Held::Choice(value.to_string()))
            }
            Kind::DateRange => {
                let day = |end: &str| param(&self.param(Some(end)))?.parse::<Date>().ok();
                let (from, to) = (day("from"), day("to"));
                (from.is_some() || to.is_some()).then_some(Held::Range { from, to })
            }
        }
    }
}

/// A value a request holds for one page filter.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Held {
    Choice(String),
    Range {
        from: Option<Date>,
        to: Option<Date>,
    },
}

/// The values the request holds for its page's [filters](PageFilter), which a widget reads:
/// [`page_filters`](crate::page_filters) returns it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageFilters {
    values: Vec<(String, Held)>,
}

impl PageFilters {
    /// The option the select `key` holds; `None` when it holds none, or the page declares no such
    /// select.
    pub fn choice(&self, key: &str) -> Option<&str> {
        self.values.iter().find_map(|(held, value)| match value {
            Held::Choice(choice) if held == key => Some(choice.as_str()),
            _ => None,
        })
    }

    /// The first and the last day the date range `key` holds, each `None` while open; both `None`
    /// when it holds neither, or the page declares no such range.
    pub fn date_range(&self, key: &str) -> (Option<Date>, Option<Date>) {
        self.values
            .iter()
            .find_map(|(held, value)| match value {
                Held::Range { from, to } if held == key => Some((*from, *to)),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Whether the request holds no value for any of the page's filters.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// What `query`, a request's query string, holds for `filters`.
pub(crate) fn read(filters: &[PageFilter], query: &str) -> PageFilters {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    PageFilters {
        values: filters
            .iter()
            .filter_map(|filter| Some((filter.key.clone(), filter.read(&pairs)?)))
            .collect(),
    }
}

#[cfg(test)]
mod tests;
