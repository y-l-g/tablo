//! [`PageDef::filter`](crate::PageDef::filter): the values a page offers above its widgets, held
//! in its query string, and [`page_filters`], which its widgets read them through.

use jiff::civil::Date;
use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use crate::{IntoOptions, Text};

/// A value a page offers above its widgets and keeps in its query string, which its widgets read
/// through [`page_filters`]: the period a dashboard's figures cover, or the store they count.
///
/// A select holds `?f.<key>=<value>`, one of its options; a date range holds `?f.<key>.from=` and
/// `?f.<key>.to=`, each a `YYYY-MM-DD` day. The bar is a form the page submits to itself, so a
/// filtered dashboard is a link, and it works without JavaScript.
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
    label: Text,
    kind: Kind,
}

/// What a page filter holds.
#[derive(Clone)]
enum Kind {
    /// One of these `(value, label)` options.
    Select(Vec<(String, Text)>),
    /// A first and a last day, either one open.
    DateRange,
}

impl PageFilter {
    /// A select over `options`, held in `?f.<key>=`: values that are their own label, `(value,
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

    /// A range of days, held in `?f.<key>.from=` and `?f.<key>.to=`. A day that does not parse
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
                let value = param(&format!("f.{}", self.key))?;
                options
                    .iter()
                    .any(|(option, _)| option == value)
                    .then(|| Held::Choice(value.to_string()))
            }
            Kind::DateRange => {
                let day = |end: &str| {
                    param(&format!("f.{}.{end}", self.key))?
                        .parse::<Date>()
                        .ok()
                };
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
/// [`page_filters`] returns it.
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

/// The bar of `filters` holding `held`: a form submitting to `url`, the page, with a reset link
/// once it holds a value. `None` without a filter.
pub(crate) fn bar<'a>(
    cx: &'a Cx,
    filters: &[PageFilter],
    held: &PageFilters,
    url: &str,
) -> Option<BoxView<'a>> {
    if filters.is_empty() {
        return None;
    }
    let controls: Vec<BoxView<'a>> = filters
        .iter()
        .map(|filter| control(cx, filter, held))
        .collect();
    let reset = (!held.is_empty()).then(|| url.to_string());
    let url = url.to_string();
    Some(
        view! {
            cx =>
            <form
                method="get"
                action=(url)
                class="flex flex-wrap items-end gap-3"
                data-page-filters=""
            >
                for control in controls {
                    (control)
                }
                <button
                    type="submit"
                    class=(tablo_ui::button_variants(
                        tablo_ui::ButtonVariant::Outline,
                        tablo_ui::ButtonSize::Md,
                    ))
                >
                    (crate::t!(cx, "tablo-page-filters-apply"))
                </button>
                if let Some(reset) = reset {
                    <a
                        href=(reset)
                        class="text-sm text-muted-foreground hover:text-foreground"
                        data-page-filters-reset=""
                    >
                        (crate::t!(cx, "tablo-page-filters-reset"))
                    </a>
                }
            </form>
        }
        .boxed(),
    )
}

/// The labelled control of `filter`, showing what `held` holds for it.
fn control<'a>(cx: &'a Cx, filter: &PageFilter, held: &PageFilters) -> BoxView<'a> {
    let label = filter.label.localize(cx);
    let key = filter.key.clone();
    let field = "flex flex-col gap-1.5 text-sm";
    let caption = "font-medium text-foreground";
    match &filter.kind {
        Kind::Select(options) => {
            let chosen = held.choice(&filter.key).map(str::to_string);
            let options: Vec<(String, String, bool)> = options
                .iter()
                .map(|(value, text)| {
                    let selected = chosen.as_deref() == Some(value.as_str());
                    (value.clone(), text.localize(cx), selected)
                })
                .collect();
            view! {
                cx =>
                <label class=(field) data-page-filter=(key.clone())>
                    <span class=(caption)>(label)</span>
                    tablo_ui::select(
                        attrs: topcoat::view::attributes! { name=(format!("f.{key}")) },
                        <option value="" selected=(chosen.is_none())>
                            (crate::t!(cx, "tablo-filter-all"))
                        </option>
                        for (value, text, selected) in options {
                            <option value=(value) selected=(selected)>(text)</option>
                        }
                    )
                </label>
            }
            .boxed()
        }
        Kind::DateRange => {
            let (from, to) = held.date_range(&filter.key);
            let day = |date: Option<Date>| date.map(|date| date.to_string()).unwrap_or_default();
            let (from, to) = (day(from), day(to));
            let from_label = crate::t!(cx, "tablo-page-filter-from", label = label.clone());
            let to_label = crate::t!(cx, "tablo-page-filter-to", label = label.clone());
            view! {
                cx =>
                <fieldset class=(field) data-page-filter=(key.clone())>
                    <legend class=(caption)>(label)</legend>
                    <span class="flex items-center gap-2">
                        tablo_ui::input(
                            attrs: topcoat::view::attributes! {
                                type="date"
                                name=(format!("f.{key}.from"))
                                value=(from)
                                aria-label=(from_label)
                            }
                        )
                        <span aria-hidden="true">"–"</span>
                        tablo_ui::input(
                            attrs: topcoat::view::attributes! {
                                type="date"
                                name=(format!("f.{key}.to"))
                                value=(to)
                                aria-label=(to_label)
                            }
                        )
                    </span>
                </fieldset>
            }
            .boxed()
        }
    }
}

/// The values the request holds for the filters of the page it renders: the registered
/// [`Page`](crate::Page) whose URL its path names. A widget reads them to narrow what it shows.
/// Outside such a page, as in a resource list's widgets, it holds none.
///
/// ```rust,no_run
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Order { #[key] #[auto] id: uuid::Uuid, channel: String }
/// # struct OrderResource;
/// # impl tablo_core::Resource for OrderResource {
/// #     type Model = Order;
/// #     type Form = tablo_core::NoForm<Order>;
/// #     fn declare() -> tablo_core::ResourceDef<Self> { tablo_core::ResourceDef::new() }
/// # }
/// use tablo_core::{Stat, Widget, db, page_filters, scoped_query};
/// use topcoat::{Result, context::Cx, view::View};
///
/// struct Orders;
///
/// impl Widget for Orders {
///     async fn render(cx: &Cx) -> Result<impl View> {
///         let mut orders = scoped_query::<OrderResource>(cx)?;
///         if let Some(channel) = page_filters(cx).choice("channel") {
///             orders = orders.filter(Order::fields().channel().eq(channel));
///         }
///         let count = db::count(cx, orders).await?;
///         Ok(tablo_core::stats(
///             cx,
///             vec![Stat::new("Orders", count.to_string())],
///         ))
///     }
/// }
/// ```
pub fn page_filters(cx: &Cx) -> PageFilters {
    match crate::panel::current_page(cx) {
        Some(page) => read(&page.filters, &crate::table::request_query(cx)),
        None => PageFilters::default(),
    }
}

#[cfg(test)]
mod tests;
