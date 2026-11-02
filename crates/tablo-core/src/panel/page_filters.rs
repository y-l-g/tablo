//! A page's filter bar, and [`page_filters`], which its widgets read the bar's values through.

use jiff::civil::Date;
use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use crate::page::filters::{self, PageFilter, PageFilters};

/// The bar of `filters` holding `held`: a form submitting to `url`, the page, with a reset link
/// once it holds a value. Both carry the request's other parameters, so a table on the page keeps
/// its search, sort and filters. `None` without a filter.
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
    let kept: Vec<(String, String)> =
        form_urlencoded::parse(crate::table::request_query(cx).as_bytes())
            .into_owned()
            .filter(|(name, _)| !name.starts_with(filters::PREFIX))
            .collect();
    let reset = (!held.is_empty()).then(|| {
        if kept.is_empty() {
            url.to_string()
        } else {
            let query = form_urlencoded::Serializer::new(String::new())
                .extend_pairs(&kept)
                .finish();
            format!("{url}?{query}")
        }
    });
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
                for (name, value) in kept {
                    <input type="hidden" name=(name) value=(value) />
                }
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
    let key = filter.key().to_string();
    let field = "flex flex-col gap-1.5 text-sm";
    let caption = "font-medium text-foreground";
    match &filter.kind {
        filters::Kind::Select(options) => {
            let name = filter.param(None);
            let chosen = held.choice(filter.key()).map(str::to_string);
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
                        attrs: topcoat::view::attributes! { name=(name) },
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
        filters::Kind::DateRange => {
            let (from_name, to_name) = (filter.param(Some("from")), filter.param(Some("to")));
            let (from, to) = held.date_range(filter.key());
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
                                name=(from_name)
                                value=(from)
                                aria-label=(from_label)
                            }
                        )
                        <span aria-hidden="true">"–"</span>
                        tablo_ui::input(
                            attrs: topcoat::view::attributes! {
                                type="date"
                                name=(to_name)
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
    match super::current_page(cx) {
        Some(page) => filters::read(&page.filters, &crate::table::request_query(cx)),
        None => PageFilters::default(),
    }
}
